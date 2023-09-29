use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker, RawWakerVTable, RawWaker};
use core::ptr::null;
use core::mem;
use core::marker::PhantomData;
use core::any::Any;

struct Node<T> {
    links: Option<(*mut Node<T>, *mut Node<T>)>,
    payload: Option<*mut T>,
}

impl<T> Node<T> {
    const fn new() -> Node<T> {
        Node {
            links: None,
            payload: None,
        }
    }
    
    fn set_next(&mut self, new_next: *mut Node<T>) {
        if let Some((prev, _)) = self.links {
            self.links = Some((prev, new_next));
        }
    }
    
    fn set_prev(&mut self, new_prev: *mut Node<T>) {
        if let Some((_, next)) = self.links {
            self.links = Some((new_prev, next));
        }
    }
    
    fn to_obj<'a>(&mut self) -> &'a mut T {
        let ptr = self.payload.take().unwrap();
        unsafe { &mut *ptr }
    } 
}

impl<T> Drop for Node<T> {
    fn drop(&mut self) {
        assert!(self.links.is_none());
    }
}

trait Linkable: Sized { 
    fn to_links(&mut self) -> &mut Node<Self>;
}

struct List<'a, T: Linkable> {
    root: Node<T>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Linkable> List<'a, T> {
    const fn new() -> List<'a, T> {
        List {
            root: Node::new(),
            _marker: PhantomData
        }
    }

    fn init(&mut self) {
        let this = (&mut self.root) as *mut Node<T>;
        self.root.links = Some((this, this));
    }

    fn get_head(&self) -> Option<&mut Node<T>> {
        match self.root.links { 
            Some((_, next)) => 
                if next as *const Node<T> != &self.root { 
                    unsafe { Some(&mut *next) } 
                } else { 
                    None 
                },
            _ => None
        }       
    }
    
    fn is_empty(&self) -> bool {
        self.get_head().is_none()
    }
    
    fn push<'b>(&mut self, object: &'b mut T) {
        let ptr: *mut T = object;
        let node = object.to_links();
        let (prev, next) = self.root.links.take().unwrap();
        node.links = Some((prev, &mut self.root));
        self.root.links = Some((node, next));
        unsafe { (*prev).set_next(node); }
        node.payload = Some(ptr);
    }

    fn pop<'b>(&mut self) -> Option<&'b mut T> {
        if let Some(node) = self.get_head() {
            let (prev, next) = node.links.take().unwrap();

            unsafe {
                (*prev).set_next(next);
                (*next).set_prev(prev);
            }
            
            return Some(node.to_obj());
        }
        None
    }
}

impl<'a, T: Linkable> Drop for List<'a, T> {
    fn drop(&mut self) {
        assert!(self.get_head().is_none());
        self.root.links = None;
    }
}

//----------------------------------------------------------------------

struct Queue<'a, T: Linkable> {
    msgs: List<'a, T>,
    subscribers: List<'a, Actor>,
    sched_context: Option<&'a mut Scheduler<'a, NPRIO>>
}

macro_rules! queue_init {
    () => { 
        Queue { 
            msgs: List::new(), 
            subscribers: List::new(), 
            sched_context: None 
        } 
    };    
}

impl<'a, T: Linkable> Queue<'a, T> {
    fn init(&mut self, sched: &'a mut Scheduler<'a, NPRIO>) {
        self.msgs.init();
        self.subscribers.init();
        self.sched_context = Some(sched);
    }
    
    fn get(&mut self) -> Option<&'a mut T> {
        self.msgs.pop()
    }
    
    fn put(&mut self, n: &'static mut T) {
        if self.subscribers.is_empty() {
            self.msgs.push(n);
        } else {
            let actor = self.subscribers.pop().unwrap();
            actor.mailbox = Some(n);
            println!("activation");
            let sched = self.sched_context.take().unwrap();
            sched.runq[actor.prio].push(actor);
            self.sched_context = Some(sched);
        }
    }
    
    fn is_empty(&self) -> bool {
        self.msgs.is_empty()
    }
}

struct Actor {
    prio: usize,
    mailbox: Option<&'static mut dyn Any>,
    future: Option<Pin<&'static mut dyn Future<Output = ()>>>,
    linkage: Node<Self>
}

macro_rules! actor_init {
    () => { 
        Actor { 
            prio: 0, 
            mailbox: None, 
            future: None, 
            linkage: Node::new() 
        } 
    };
}

impl Actor {
    fn block_on<'a, 'b, T: Linkable>(&'a mut self, q: &'a mut Queue<'b, T>) -> MsgFuture<'a, 'b, T> {
        MsgFuture::<T>::new(q, self)
    }
}

impl Linkable for Actor {
    fn to_links(&mut self) -> &mut Node<Self> {
        &mut self.linkage
    }
}

impl Actor {
    fn call(&mut self) {   
        let waker = waker_ref();
        let mut cx = std::task::Context::from_waker(waker);    
        let mut p = self.future.take().unwrap();    
        let _ = p.as_mut().poll(&mut cx);
        self.future = Some(p);
    }    

    fn spawn(&mut self, f: &mut (impl Future<Output=()> + 'static)) {
        fn to_static<'a, T>(v: &'a mut T) -> &'static mut T {
            unsafe { mem::transmute(v) }
        }        
        
        let s = to_static(f);
        unsafe { self.future = Some(Pin::new_unchecked(s)); }
        self.call();
    }
    
}

struct MsgFuture<'a, 'b, T: Linkable> {
    queue: &'a mut Queue<'b, T>,
    actor: Option<&'a mut Actor>
}

impl<'a, 'b, T: Linkable> MsgFuture<'a, 'b, T> {
    fn new(q: &'a mut Queue<'b, T>, a: &'a mut Actor) -> MsgFuture<'a, 'b, T> {
        MsgFuture { queue: q, actor: Some(a) }
    }    
}

impl<'a, 'b: 'a, T: Linkable + 'static> Future for MsgFuture<'a, 'b, T> {
    type Output = &'b mut T;
    
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        let a = self.actor.take().unwrap();
        if a.mailbox.is_some() {
            let any_msg = a.mailbox.take().unwrap();
            let msg = any_msg.downcast_mut::<T>().unwrap();
            println!("from actor");
            return Poll::Ready(msg);
        } else {
            if self.queue.is_empty() {
                println!("poll returns pending");
                self.queue.subscribers.push(a);
                self.actor = Some(a);
                return Poll::Pending;
            } else {
                println!("poll returns ready");
                return Poll::Ready(self.queue.get().unwrap());
            }
        }
    }
}

struct Message<'a, T> {
    parent: Option<&'a mut Queue<'a, Self>>,
    payload: T,
    linkage: Node<Self>
}

impl<'a, T> Linkable for Message<'a, T> {
    fn to_links(&mut self) -> &mut Node<Self> {
        &mut self.linkage
    }
}

trait Freeable {
    fn free(&mut self);
    fn set_parent(&mut self, _: &'static mut Queue<'static, Self>) 
        where Self: Linkable;
}

impl<'a, T> Freeable for Message<'a, T> {
    fn free(&mut self) {       
        if let Some(parent) = self.parent.take() {
            parent.msgs.push(self);
        }
    }
    
    fn set_parent(&mut self, q: &'static mut Queue<'static, Self>) {
        self.parent = Some(q);
    }
}

struct Pool<'a, T: Linkable, const N: usize> {
    pool: Queue<'a, T>,
    used: usize,
    arr: Option<&'a mut [T; N]>
}

impl<'a: 'static, T: Freeable + Linkable, const N: usize> Pool<'a, T, N> {
    
    fn init(&'a mut self, mem: &'a mut [T; N], sched: &'a mut Scheduler<'a, NPRIO>) {
        self.pool.init(sched);
        self.used = 0;
        self.arr = Some(mem);
    }
    
    fn alloc(&'a mut self) -> Option<&'a mut T> {
        if self.used < N {
            let val = match &mut self.arr {
                Some(arr_ref) => Some(&mut arr_ref[self.used]),
                _ => None
            };
            self.used += 1;
            if val.is_some() {
                let x = val.unwrap();
                x.set_parent(&mut self.pool);
                Some(x)
            } else {
                None
            }
        } else {
            if self.pool.msgs.is_empty() == false {
                self.pool.msgs.pop()
            } else {
                None
            }
        }
    }
}

struct Scheduler<'a, const NPRIO: usize> {
    runq: [List<'a, Actor>; NPRIO]
}

impl<'a, const NPRIO: usize> Scheduler<'a, NPRIO> {
    const fn new() -> Self {
        const RUNQ_PROTO: List::<Actor> = List::<Actor>::new();
        Self { runq: [ RUNQ_PROTO; NPRIO ] }
    }
    
    fn init(&mut self) {
        for i in 0..NPRIO {
            self.runq[i].init()
        }
    }
    
    fn schedule(&mut self, vect: usize) {       
        let s = &mut self.runq[vect];
        while s.is_empty() == false {
            let a = s.pop().unwrap();
            a.call();           
        }
    }    
}

const fn noop_raw_waker() -> RawWaker {
    RawWaker::new(null(), &NOOP_WAKER_VTABLE)
}

const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| noop_raw_waker(), |_| {}, |_| {}, |_| {}
);

fn waker_ref() -> &'static Waker {
    struct SyncRawWaker(RawWaker);
    unsafe impl Sync for SyncRawWaker {}
    static NOOP_WAKER_INSTANCE: SyncRawWaker = SyncRawWaker(noop_raw_waker());
    unsafe { &*(&NOOP_WAKER_INSTANCE.0 as *const RawWaker as *const Waker) }
}

const NPRIO: usize = 1;
const PROTO: Message<ExampleMsg> = Message { parent: None, payload: ExampleMsg { n: 0 }, linkage: Node::new() };
const PROTO2: Message<ExampleMsg2> = Message { parent: None, payload: ExampleMsg2 { t: 0 }, linkage: Node::new() };
static mut ARR: [Message<ExampleMsg>; 2] = [PROTO; 2];
static mut ARR2: [Message<ExampleMsg2>; 2] = [PROTO2; 2];
static mut POOL: Pool<Message<ExampleMsg>, 2> = Pool { pool: queue_init!(), used: 0, arr: None };
static mut POOL2: Pool<Message<ExampleMsg2>, 2> = Pool { pool: queue_init!(), used: 0, arr: None };
static mut QUEUE: Queue<Message<ExampleMsg>> = queue_init!();
static mut QUEUE2: Queue<Message<ExampleMsg2>> = queue_init!();

struct ExampleMsg {
    n: u32
}

struct ExampleMsg2 {
    t: u32
}

async fn func(this: &mut Actor) -> () {
    let q = unsafe { &mut QUEUE };
    let q2 = unsafe { &mut QUEUE2 };
    let mut foo = 0;
    
    loop {
        println!("before 1st await {}", foo);
        let t = this.block_on(q).await;
        foo += t.payload.n;
        t.free();
        println!("before 2nd await {}", foo);
        let m = this.block_on(q2).await;
        foo += m.payload.t;
        m.free();
        println!("loop end {}", foo);
    }
}

fn get_size<F: Future>(_: &mut F) -> usize {
    core::mem::size_of::<F>()
}

fn main() {
    static mut ACTOR1: Actor = actor_init!();
    static mut SCHED: Scheduler<NPRIO> = Scheduler::new();
    
    unsafe { 
        SCHED.init();
        QUEUE.init(&mut SCHED);
        QUEUE2.init(&mut SCHED);
        POOL.init(&mut ARR, &mut SCHED);
        POOL2.init(&mut ARR2, &mut SCHED);
    
        let mut f = func(&mut ACTOR1);
        println!("future size = {}", get_size(&mut f));

        ACTOR1.spawn(&mut f);
        
        let msg = POOL.alloc().unwrap();
        msg.payload.n = 100;
        QUEUE.put(msg);
        
        SCHED.schedule(0);
        
        let msg = POOL2.alloc().unwrap();
        msg.payload.t = 10;
        QUEUE2.put(msg);
        
        SCHED.schedule(0);
        
        let msg = POOL.alloc().unwrap();
        msg.payload.n = 1;
        QUEUE.put(msg);
        
        SCHED.schedule(0); 
    }
}
