use core::mem;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker, RawWakerVTable, RawWaker};
use core::ptr::null;
use core::marker::PhantomData;
use core::any::Any;
use core::cell::{Cell, OnceCell};
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

struct Ref<'a, T>(Option<&'a mut T>);

impl<'a, T> Ref<'a, T> {
    fn new(r: &'a mut T) -> Self {
        Self (Some(r))
    }
       
    fn release(mut self) -> &'a mut T {
        self.0.take().unwrap()
    }
}

struct Node<T> {
    links: Cell<Option<(*const Node<T>, *const Node<T>)>>,
    payload: Cell<Option<*mut T>>,
}

impl<T> Node<T> {
    const fn new() -> Node<T> {
        Node {
            links: Cell::new(None),
            payload: Cell::new(None),
        }
    }
    
    fn set_next(&self, new_next: *const Node<T>) {
        if let Some((prev, _)) = self.links.take() {
            self.links.set(Some((prev, new_next)));
        }
    }
    
    fn set_prev(&self, new_prev: *const Node<T>) {
        if let Some((_, next)) = self.links.take() {
            self.links.set(Some((new_prev, next)));
        }
    }
    
    fn unlink(&self) {   
        if let Some((prev, next)) = self.links.take() {
            unsafe {
                (*prev).set_next(next);
                (*next).set_prev(prev);
            }
        }
    }

    fn to_obj<'a>(&self) -> Ref<'a, T> {
        let ptr = self.payload.take().unwrap();
        unsafe { Ref::new(&mut *ptr) }
    }
}

trait Linkable: Sized {
    fn to_links(&self) -> &Node<Self>;
}

struct List<'a, T: Linkable> {
    root: Node<T>,
    _marker: PhantomData<Cell<&'a T>>,
}

impl<'a, T: Linkable> List<'a, T> {
    const fn new() -> List<'a, T> {
        List {
            root: Node::new(),
            _marker: PhantomData
        }
    }

    fn init(&self) {
        let this = &self.root as *const Node<T>;
        self.root.links.set(Some((this, this)));
    }

    fn peek_head_node(&self) -> Option<&Node<T>> {
        match self.root.links.get() { 
            Some((_, next)) => 
                if next != &self.root { 
                    unsafe { Some(&*next) } 
                } else { 
                    None 
                },
            _ => None
        }
    }
    
    fn is_empty(&self) -> bool {
        self.peek_head_node().is_none()
    }
    
    fn enqueue(&self, wrapper: Ref<'a, T>) {
        let object = wrapper.release();
        let ptr: *mut T = object;
        let node = object.to_links();
        let (prev, next) = self.root.links.take().unwrap();
        node.links.set(Some((prev, &self.root)));
        node.payload.set(Some(ptr));
        self.root.links.set(Some((node, next)));
        unsafe { (*prev).set_next(node); }
    }

    fn dequeue(&self) -> Option<Ref<'a, T>> {
        if let Some(node) = self.peek_head_node() {
            node.unlink();
            Some(node.to_obj())
        } else {
            None
        }
    }
}

struct Message<'a, T> {
    parent: OnceCell<&'a Queue<'a, T>>,
    linkage: Node<Self>,
    payload: T
}

impl<'a, T> Message<'a, T> {
    const fn new(payload: T) -> Self {
        Self { 
            parent: OnceCell::new(), 
            linkage: Node::new(), 
            payload: payload
        }        
    }
}

impl<'a, T> Linkable for Message<'a, T> {
    fn to_links(&self) -> &Node<Self> {
        &self.linkage
    }
}

struct Envelope<'a, T> {
    inner: Ref<'a, Message<'a, T>>
}

impl<'a, T> Envelope<'a, T> {    
    fn from_wrapper(wrapper: Option<Ref<'a, Message<'a, T>>>) -> Option<Self> {
        wrapper.map(|msg| Self { inner: Ref::new(msg.release()) })
    }
    
    fn into_wrapper(mut self) -> Ref<'a, Message<'a, T>> {
        Ref::new(self.inner.0.take().unwrap())
    }
}

impl<'a, T> Deref for Envelope<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner.0.as_deref().unwrap().payload
    }
}

impl<'a, T> DerefMut for Envelope<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner.0.as_deref_mut().unwrap().payload
    }
}

impl<'a, T> Drop for Envelope<'a, T> {
    fn drop(&mut self) {
        if let Some(msg) = self.inner.0.take() {
            let parent = msg.parent.get().unwrap();
             parent.msgs.enqueue(Ref::new(msg));
        }
    }
}

struct Queue<'a, T: Sized> {
    msgs: List<'a, Message<'a, T>>,
    subscribers: List<'a, Actor>
}

impl<'a: 'static, T: Sized> Queue<'a, T> {
    const fn new() -> Self {
        Self { 
            msgs: List::new(), 
            subscribers: List::new(),
        }         
    }
    
    fn init(&self) {
        self.msgs.init();
        self.subscribers.init();
    }
        
    fn get(&self, actor: Ref<'a, Actor>) -> Option<Envelope<'a, T>> {
        if self.msgs.is_empty() {
            self.subscribers.enqueue(actor);
            None
        } else {
            Envelope::from_wrapper(self.msgs.dequeue())
        }
    }
    
    fn put(&self, msg: Envelope<'a, T>) {
        if self.subscribers.is_empty() {
            self.msgs.enqueue(msg.into_wrapper());
        } else {
            let actor = self.subscribers.dequeue().unwrap();
            Actor::set(actor, msg);
        }
    }
}

struct Pool<'a, T: Sized, const N: usize> {
    pool: Queue<'a, T>,
    used: Cell<usize>,
    slice: Cell<Option<&'a mut [Message<'a, T>]>>
}

macro_rules! pool_init {
    () => { 
        Pool { 
            pool: Queue::new(), 
            used: Cell::new(0), 
            slice: Cell::new(None) 
        } 
    };
}

impl<'a: 'static, T: Sized, const N: usize> Pool<'a, T, N> {
    fn init(&self, arr: &'a mut [Message<'a, T>; N]) {
        self.pool.init();
        self.slice.set(Some(&mut arr[0..N]));
    }
    
    fn alloc(&'a self) -> Option<Envelope<'a, T>> {
        let used = self.used.get();
        let msg = if used < N {
            let remaining_items = self.slice.take().unwrap();
            let (item, rest) = remaining_items.split_at_mut(1);
            self.used.set(used + 1);
            self.slice.set(Some(rest));
            let _ = item[0].parent.set(&self.pool);
            Some(Ref::new(&mut item[0]))
        } else {
            self.pool.msgs.dequeue()
        };
        
        Envelope::from_wrapper(msg)
    }
}

type PinnedFuture = Pin<&'static mut dyn Future<Output = ()>>;

#[repr(C)]
struct Actor {
    waker: RawWaker,
    prio: usize,
    future_id: Option<usize>,
    mailbox: Option<&'static mut dyn Any>,
    context: Option<&'static Executor<'static>>,
    linkage: Node<Self>
}

macro_rules! actor_new {
    ($n:expr) => { 
        Actor {
            waker: noop_raw_waker(),
            prio: $n, 
            future_id: None,
            mailbox: None, 
            context: None,
            linkage: Node::new() 
        }
    };
}

impl Linkable for Actor {
    fn to_links(&self) -> &Node<Self> {
        &self.linkage
    }
}

const fn noop_raw_waker() -> RawWaker {
    RawWaker::new(null(), &NOOP_WAKER_VTABLE)
}

const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| noop_raw_waker(), |_| {}, |_| {}, |_| {}
);

fn waker_ref(waker: &mut RawWaker) -> &Waker {
    unsafe { &*(waker as *const RawWaker as *const Waker) }
}

impl Actor {   
    fn call(&mut self, f: &mut OnceCell<PinnedFuture>) {
        let waker = waker_ref(&mut self.waker);
        let mut cx = core::task::Context::from_waker(waker);
        let future = f.get_mut().unwrap();
        let _ = future.as_mut().poll(&mut cx);
    }
    
    fn set<'a: 'static, T>(wrapper: Ref<'a, Actor>, msg: Envelope<'a, T>) {
        let actor = wrapper.release();
        let exec = actor.context.take().unwrap();
        actor.mailbox = Some(msg.into_wrapper().release());
        exec.activate(actor.prio, Ref::new(actor));
    }    
}

impl<'q: 'static, T> Future for &Queue<'q, T> {
    type Output = Envelope<'q, T>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let waker = cx.waker() as *const Waker as *mut Waker;
        let actor = unsafe { &mut *(waker as *mut Actor) };
        if let Some(any_ref) = actor.mailbox.take() {
            let msg = any_ref.downcast_mut::<Message<'q, T>>().map(
                |msg| Ref::new(msg)
            );
            let env = Envelope::from_wrapper(msg).unwrap();
            Poll::Ready(env)            
        } else {
            match self.get(Ref::new(actor)) {
                Some(msg) =>  Poll::Ready(msg),
                None => Poll::Pending
            }
        }
    }
}

struct ArrayAccessor<'a, T> {
    ptr: *mut T,
    len: usize,
    _marker: PhantomData<&'a mut T>
}

impl<'a, T> ArrayAccessor<'a, T> {
    fn new(slice: &'a mut [T]) -> ArrayAccessor<'a, T> {
        Self {
            ptr: slice.as_mut_ptr(),
            len: slice.len(),
            _marker: PhantomData
        }
    }

    unsafe fn as_mut_item(&self, i: usize) -> &'a mut T {
        assert!(i < self.len);
        &mut *self.ptr.add(i)
    }
}

const NPRIO: usize = 1;
const EMPTY_CELL: OnceCell<PinnedFuture> = OnceCell::new();

struct Executor<'a> {
    runq: [List<'a, Actor>; NPRIO],
    futures: OnceCell<ArrayAccessor<'a, OnceCell<PinnedFuture>>>,
    ticket: AtomicUsize
}

macro_rules! executor_new {
    () => {
        Executor { 
            runq: [ List::<Actor>::new(); NPRIO ], 
            futures: OnceCell::new(), 
            ticket: AtomicUsize::new(0) 
        } 
    }
}

impl<'a: 'static> Executor<'a> {
    fn init(&self, arr: &'a mut [OnceCell<PinnedFuture>]) {
        let _ = self.futures.set(ArrayAccessor::new(arr));
        for i in 0..NPRIO {
            self.runq[i].init()
        }
    }

    fn schedule(&'a self, vect: usize) {       
        let runq = &self.runq[vect];
        while runq.is_empty() == false {
            let wrapper = runq.dequeue().unwrap();
            let actor = wrapper.release();
            let fut_id = actor.future_id.as_ref().copied().unwrap();
            let futures = self.futures.get().unwrap();
            let f = unsafe { futures.as_mut_item(fut_id) };
            actor.context = Some(self);
            actor.call(f);
        }
    }
    
    fn activate(&'a self, prio: usize, wrapper: Ref<'a, Actor>) {
        self.runq[prio].enqueue(wrapper);
    }
    
    fn spawn(&'a self, actor: &mut Actor, f: &mut (impl Future<Output=()> + 'static)) {
        fn to_static<'x, T>(v: &'x mut T) -> &'static mut T {
            unsafe { mem::transmute(v) }
        }        
        
        let static_fut = to_static(f);
        let pinned_fut = unsafe { Pin::new_unchecked(static_fut) };
        let fut_id = self.ticket.fetch_add(1, Ordering::SeqCst);
        let futures = self.futures.get().unwrap();
        let f = unsafe { futures.as_mut_item(fut_id) };
        let _ = f.set(pinned_fut);
        actor.future_id = Some(fut_id);
        actor.context = Some(self);
        actor.call(f);
    }    
}

//----------------------------------------------------------------------

struct ExampleMsg {
    n: u32
}

struct ExampleMsg2 {
    t: u32
}

async fn func<'a: 'static>(
    q1: &'a Queue<'a, ExampleMsg>, 
    q2: &'a Queue<'a, ExampleMsg2>) {

    let mut sum = 0;
        
    loop {
        println!("before 1st await {}", sum);
        
        let msg1 = &q1.await;
        sum += msg1.n;
        
        println!("before 2nd await {}", sum);
        
        let msg2 = &q2.await;
        sum += msg2.t;
    }
}

fn main() {
    static mut POOL1: Pool<ExampleMsg, 2> = pool_init!();
    static mut POOL2: Pool<ExampleMsg2, 2> = pool_init!();
    static mut QUEUE1: Queue<ExampleMsg> = Queue::new();
    static mut QUEUE2: Queue<ExampleMsg2> = Queue::new();
    
    const PROTO1: Message<ExampleMsg> = Message::new(ExampleMsg { n: 0 });
    const PROTO2: Message<ExampleMsg2> = Message::new(ExampleMsg2 { t: 0 });
           
    static mut ARR1: [Message<ExampleMsg>; 2] = [PROTO1; 2];
    static mut ARR2: [Message<ExampleMsg2>; 2] = [PROTO2; 2];
    
    static mut FUTURES: [OnceCell<PinnedFuture>; 5] = [EMPTY_CELL; 5];
    static mut ACTOR1: Actor = actor_new!(0);
    static mut SCHED: Executor = executor_new!();
    
    unsafe { 
        SCHED.init(FUTURES.as_mut_slice());
        QUEUE1.init();
        QUEUE2.init();
        POOL1.init(&mut ARR1);
        POOL2.init(&mut ARR2);
    
        let mut f = func(&QUEUE1, &QUEUE2);
        SCHED.spawn(&mut ACTOR1, &mut f);
        
        for _ in 0..10 {
            let mut msg = POOL1.alloc().unwrap();
            msg.n = 10;
            QUEUE1.put(msg);
            
            SCHED.schedule(0);
            
            let mut msg = POOL2.alloc().unwrap();
            msg.t = 100;
            QUEUE2.put(msg);
            
            SCHED.schedule(0);
        }
    }
}
