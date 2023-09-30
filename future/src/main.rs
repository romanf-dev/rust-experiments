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
        assert!(self.links.is_none());
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

    fn peek_head_node(&self) -> Option<&mut Node<T>> {
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
        self.peek_head_node().is_none()
    }
    
    fn push<'b>(&mut self, object: &'b mut T) {
        let ptr: *mut T = object;
        let node = object.to_links();
        let (prev, next) = self.root.links.take().unwrap();
        node.links = Some((prev, &mut self.root));
        node.payload = Some(ptr);
        self.root.links = Some((node, next));
        unsafe { (*prev).set_next(node); }
    }

    fn pop<'b>(&mut self) -> Option<&'b mut T> {
        if let Some(node) = self.peek_head_node() {
            let (prev, next) = node.links.take().unwrap();

            unsafe {
                (*prev).set_next(next);
                (*next).set_prev(prev);
            }
            
            Some(node.to_obj())
        } else {
            None
        }
    }
}

impl<'a, T: Linkable> Drop for List<'a, T> {
    fn drop(&mut self) {
        assert!(self.peek_head_node().is_none());
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
        
    fn get<'b>(&mut self, actor: &'b mut Actor) -> Option<&'a mut T> {
        if self.msgs.is_empty() {
            self.subscribers.push(actor);
            None
        } else {
            self.msgs.pop()
        }
    }
    
    fn put(&mut self, item: &'static mut T) {
        if self.subscribers.is_empty() {
            self.msgs.push(item);
        } else {
            let actor = self.subscribers.pop().unwrap();
            actor.mailbox = Some(item);
            
            if let Some(ref mut sched) = self.sched_context {
                sched.runq[actor.prio].push(actor)
            }
        }
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

impl Linkable for Actor {
    fn to_links(&mut self) -> &mut Node<Self> {
        &mut self.linkage
    }
}

impl Actor {
    fn call(&mut self) {   
        let waker = waker_ref();
        let mut cx = core::task::Context::from_waker(waker);
        
        if let Some(ref mut future) = self.future {
            let _ = future.as_mut().poll(&mut cx);
        }
    }    

    fn spawn(&mut self, f: &mut (impl Future<Output=()> + 'static)) {
        fn to_static<'x, T>(v: &'x mut T) -> &'static mut T {
            unsafe { mem::transmute(v) }
        }        
        
        let s = to_static(f);
        unsafe { self.future = Some(Pin::new_unchecked(s)); }
        self.call();
    }
    
    fn block_on<'refs, 'content, T: Linkable>
        (&'refs mut self, q: &'refs mut Queue<'content, T>) -> MsgFuture<'refs, 'content, T> {

        MsgFuture::<T> { queue: Some(q), actor: self }
    }
}

struct MsgFuture<'a, 'q, T: Linkable> {
    queue: Option<&'a mut Queue<'q, T>>,
    actor: &'a mut Actor
}

impl<'a, 'q, T: Linkable + 'static> Future for MsgFuture<'a, 'q, T> {
    type Output = &'q mut T;
    
    fn poll(mut self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        if let Some(queue) = self.queue.take() {
            match queue.get(self.actor) {
                Some(msg) =>  Poll::Ready(msg),
                None => Poll::Pending
            }           
        } else {
            let msg = self.actor.mailbox.take().unwrap();
            Poll::Ready(msg.downcast_mut::<T>().unwrap())
        }
    }
}

struct Message<'a, T> {
    parent: Option<&'a mut Queue<'a, Self>>,
    linkage: Node<Self>,
    payload: T
}

macro_rules! msg_new {
    ($s:expr) => { 
        Message { 
            parent: None, 
            linkage: Node::new(), 
            payload: $s
        }
    }
}
    
impl<'a, T> Linkable for Message<'a, T> {
    fn to_links(&mut self) -> &mut Node<Self> {
        &mut self.linkage
    }
}

impl<'a, T> Message<'a, T> {
    fn free(&mut self) {       
        if let Some(parent) = self.parent.take() {
            parent.msgs.push(self);
        }
    }
    
    fn set_parent(&mut self, q: &'a mut Queue<'a, Self>) {
        self.parent = Some(q);
    }
}

struct Pool<'a, T: Sized, const N: usize> {
    pool: Queue<'a, Message<'a, T>>,
    used: usize,
    arr: Option<&'a mut [Message<'a, T>; N]>
}

macro_rules! pool_init {
    () => { Pool { pool: queue_init!(), used: 0, arr: None } };
}

impl<'a, T: Sized, const N: usize> Pool<'a, T, N> {
    fn init(&mut self, mem: &'a mut [Message<'a, T>; N], sched: &'a mut Scheduler<'a, NPRIO>) {
        self.pool.init(sched);
        self.used = 0;
        self.arr = Some(mem);
    }
    
    fn alloc(&'a mut self) -> Option<&'a mut Message<'a, T>> {
        if self.used < N {
            if let Some(ref mut arr) = self.arr {
                let item = &mut arr[self.used];
                self.used += 1;
                item.set_parent(&mut self.pool);
                Some(item)
            } else {
                None
            }
        } else {
            self.pool.msgs.pop()
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
        let runq = &mut self.runq[vect];
        while runq.is_empty() == false {
            let actor = runq.pop().unwrap();
            actor.call();           
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
const PROTO1: Message<ExampleMsg> = msg_new!(ExampleMsg { n: 0 });
const PROTO2: Message<ExampleMsg2> = msg_new!(ExampleMsg2 { t: 0 });
    
static mut ARR1: [Message<ExampleMsg>; 2] = [PROTO1; 2];
static mut ARR2: [Message<ExampleMsg2>; 2] = [PROTO2; 2];
static mut POOL1: Pool<ExampleMsg, 2> = pool_init!();
static mut POOL2: Pool<ExampleMsg2, 2> = pool_init!();
static mut QUEUE1: Queue<Message<ExampleMsg>> = queue_init!();
static mut QUEUE2: Queue<Message<ExampleMsg2>> = queue_init!();

struct ExampleMsg {
    n: u32
}

struct ExampleMsg2 {
    t: u32
}

async fn func(this: &mut Actor) {
    let q1 = unsafe { &mut QUEUE1 };
    let q2 = unsafe { &mut QUEUE2 };
    let mut sum = 0;
    
    loop {
        println!("before 1st await {}", sum);
        
        let msg1 = this.block_on(q1).await;
        sum += msg1.payload.n;
        msg1.free();
        
        println!("before 2nd await {}", sum);
        
        let msg2 = this.block_on(q2).await;
        sum += msg2.payload.t;
        msg2.free();
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
        QUEUE1.init(&mut SCHED);
        QUEUE2.init(&mut SCHED);
        POOL1.init(&mut ARR1, &mut SCHED);
        POOL2.init(&mut ARR2, &mut SCHED);
    
        let mut f = func(&mut ACTOR1);
        println!("future size = {}", get_size(&mut f));

        ACTOR1.spawn(&mut f);
        
        let msg = POOL1.alloc().unwrap();
        msg.payload.n = 100;
        QUEUE1.put(msg);
        
        SCHED.schedule(0);
        
        let msg = POOL2.alloc().unwrap();
        msg.payload.t = 10;
        QUEUE2.put(msg);
        
        SCHED.schedule(0);
        
        let msg = POOL1.alloc().unwrap();
        msg.payload.n = 1;
        QUEUE1.put(msg);
        
        SCHED.schedule(0);
        
        let msg = POOL2.alloc().unwrap();
        msg.payload.t = 1000;
        QUEUE2.put(msg);
        
        SCHED.schedule(0);        
    }
}
