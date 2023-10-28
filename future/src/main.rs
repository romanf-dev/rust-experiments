use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker, RawWakerVTable, RawWaker};
use core::ptr::null;
use core::mem;
use core::marker::PhantomData;
use core::any::Any;
use core::cell::Cell;
use core::ops::DerefMut;
use core::ops::Deref;

struct Ref<'a, T>(Option<&'a mut T>);

impl<'a, T> Ref<'a, T> {
    fn new(r: &'a mut T) -> Self {
        Self (Some(r))
    }
    
    fn as_mut(&mut self) -> Option<&mut T> {
        self.0.as_deref_mut()
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
        let this = (&self.root) as *const Node<T>;
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
        node.links.set(Some((prev, &self.root as *const Node<T>)));
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
    parent: Option<&'a Queue<'a, T>>,
    linkage: Node<Self>,
    payload: T
}

impl<'a, T> Message<'a, T> {
    const fn new(payload: T) -> Self {
        Self { 
            parent: None, 
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
        match wrapper {
            Some(msg) => Some(Self { inner: Ref::new(msg.release()) }),
            _ => None
        }
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
        if let Some(r) = self.inner.0.take() {
            if let Some(ref parent) = r.parent {
                parent.msgs.enqueue(Ref::new(r));
            }
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
    
    fn init(&mut self) {
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
    
    fn block_on<'b>(&'b self) -> MsgFuture<'b, 'a, T> {
        MsgFuture::<T> { queue: Some(self) }
    }    
}

#[repr(C)]
struct Actor {
    waker: RawWaker,
    prio: usize,
    mailbox: Option<&'static mut dyn Any>,
    future: Option<Pin<&'static mut dyn Future<Output = ()>>>,
    context: Option<&'static Scheduler<'static>>,
    linkage: Node<Self>
}

macro_rules! actor_init {
    () => { 
        Actor {
            waker: noop_raw_waker(),
            prio: 0, 
            mailbox: None, 
            future: None,
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

impl Actor {
    fn call(&mut self) {   
        let waker = waker_ref(&mut self.waker);
        let mut cx = core::task::Context::from_waker(waker);
        
        if let Some(ref mut future) = self.future {
            let _ = future.as_mut().poll(&mut cx);
        }
    }
    
    fn set<'a: 'static, T>(mut wrapper: Ref<'a, Actor>, msg: Envelope<'a, T>) {
        let actor = wrapper.as_mut().unwrap();
        actor.mailbox = Some(msg.into_wrapper().release());
        let sched = actor.context.take().unwrap();
        sched.runq[actor.prio].enqueue(wrapper);
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
    
struct MsgFuture<'r, 'q, T> {
    queue: Option<&'r Queue<'q, T>>,
}

impl<'r, 'q: 'static, T> Future for MsgFuture<'r, 'q, T> {
    type Output = Envelope<'q, T>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let waker = cx.waker() as *const Waker as *mut Waker;
        let actor = unsafe { &mut *(waker as *mut Actor) };
        if let Some(queue) = self.queue.take() {
            match queue.get(Ref::new(actor)) {
                Some(msg) =>  Poll::Ready(msg),
                None => Poll::Pending
            }           
        } else {
            let any_ref = actor.mailbox.take().unwrap();
            let msg = any_ref.downcast_mut::<Message<'q, T>>().unwrap();
            let env = Envelope::from_wrapper(Some(Ref::new(msg))).unwrap();
            Poll::Ready(env)
        }
    }
}

struct Pool<'a, T: Sized, const N: usize> {
    pool: Queue<'a, T>,
    used: usize,
    slice: Option<&'a mut [Message<'a, T>]>
}

macro_rules! pool_init {
    () => { Pool { pool: Queue::new(), used: 0, slice: None } };
}

impl<'a: 'static, T: Sized, const N: usize> Pool<'a, T, N> {
    fn init(&mut self, arr: &'a mut [Message<'a, T>; N]) {
        self.pool.init();
        self.slice = Some(&mut arr[0..N]);
    }
    
    fn alloc(&'a mut self) -> Option<Envelope<'a, T>> {
        let msg = if self.used < N {
            let remaining_items = self.slice.take().unwrap();
            let (item, rest) = remaining_items.split_at_mut(1);
            self.used += 1;
            self.slice = Some(rest);
            item[0].parent = Some(&self.pool);
            Some(Ref::new(&mut item[0]))
        } else {
            self.pool.msgs.dequeue()
        };
        
        Envelope::from_wrapper(msg)
    }
}

const NPRIO: usize = 1;

struct Scheduler<'a> {
    runq: [List<'a, Actor>; NPRIO]
}

impl<'a: 'static> Scheduler<'a> {
    const fn new() -> Self {
        const RUNQ_PROTO: List::<Actor> = List::<Actor>::new();
        Self { runq: [ RUNQ_PROTO; NPRIO ] }
    }
    
    fn init(&mut self) {
        for i in 0..NPRIO {
            self.runq[i].init()
        }
    }
    
    fn schedule(&'a self, vect: usize) {       
        let runq = &self.runq[vect];
        while runq.is_empty() == false {
            let wrapper = runq.dequeue().unwrap();
            let actor = wrapper.release();
            actor.context = Some(self);
            actor.call();
        }
    }
    
    fn spawn(&'a self, actor: &mut Actor, f: &mut (impl Future<Output=()> + 'static)) {
        fn to_static<'x, T>(v: &'x mut T) -> &'static mut T {
            unsafe { mem::transmute(v) }
        }        
        
        let s = to_static(f);
        unsafe { actor.future = Some(Pin::new_unchecked(s)); }
        actor.context = Some(self);
        actor.call();
    }    
}

//----------------------------------------------------------------------

struct ExampleMsg {
    n: u32
}

struct ExampleMsg2 {
    t: u32
}

async fn func<'a: 'static>(q1: &'a Queue<'a, ExampleMsg>, q2: &'a Queue<'a, ExampleMsg2>) {
    let mut sum = 0;
        
    loop {
        println!("before 1st await {}", sum);
        
        let msg1 = q1.block_on().await;
        sum += msg1.n;
        
        println!("before 2nd await {}", sum);
        
        let msg2 = q2.block_on().await;
        sum += msg2.t;
    }
}

fn get_size<F: Future>(_: &mut F) -> usize {
    core::mem::size_of::<F>()
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
    
    static mut ACTOR1: Actor = actor_init!();
    static mut SCHED: Scheduler = Scheduler::new();
    
    unsafe { 
        SCHED.init();
        QUEUE1.init();
        QUEUE2.init();
        POOL1.init(&mut ARR1);
        POOL2.init(&mut ARR2);
    
        let mut f = func(&QUEUE1, &QUEUE2);
        println!("future size = {}", get_size(&mut f));

        SCHED.spawn(&mut ACTOR1, &mut f);
        
        for _ in 0..5 {
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
