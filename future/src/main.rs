use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::task::RawWakerVTable;
use core::task::RawWaker;
use core::ptr::null;
use core::task::Waker;
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
    subscribers: List<'a, Actor>
}

macro_rules! queue_init {
    () => { Queue { msgs: List::new(), subscribers: List::new() } };    
}

macro_rules! actor_init {
    () => { Actor { prio: 10, mailbox: None, future: None, linkage: Node::new() } };
}

impl<'a, T: Linkable> Queue<'a, T> {
    
    fn init(&mut self) {
        self.msgs.init();
        self.subscribers.init();
    }
    
    fn get(&mut self) -> Option<&'a mut T> {
        self.msgs.pop()
    }
    
    fn put(&mut self, n: &'static mut T) {
        if self.subscribers.is_empty() {
            self.msgs.push(n);
        } else {
            let a = self.subscribers.pop().unwrap();
            a.mailbox = Some(n);
            println!("activation");
            unsafe { SCHED[0].push(a); }
        }
    }
    
    fn is_empty(&self) -> bool {
        self.msgs.is_empty()
    }
}

struct Actor {
    prio: u32,
    mailbox: Option<&'static mut dyn Any>,
    future: Option<Pin<&'static mut dyn Future<Output = ()>>>,
    linkage: Node<Self>
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

struct Message {
    n: u32,
    linkage: Node<Self>
}

impl Linkable for Message {
    fn to_links(&mut self) -> &mut Node<Self> {
        &mut self.linkage
    }
}

static mut MSG: Message = Message { n: 5, linkage: Node::new() };
static mut QUEUE: Queue<Message> = queue_init!();
static mut ACTOR: [Actor; 2] = [ actor_init!(), actor_init!() ];
static mut SCHED: [List<Actor>; 2] = [ List::new(), List::new() ];

async fn func(this: &mut Actor) -> () {
    let q = unsafe { &mut QUEUE };
    
    let mut foo = this.prio;
    loop {
        println!("before await {}", foo);
        let t = this.block_on(q).await;
        foo += t.n;
        println!("after await {}", foo);
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

fn to_static<'a, T>(v: &'a mut T) -> &'static mut T {
    unsafe { mem::transmute(v) }
}

fn print_sizeof<F: Future>(_: &mut F) -> usize {
    core::mem::size_of::<F>()
}

fn call_once(a: &mut Actor) {   
    let waker = waker_ref();
    let mut cx = std::task::Context::from_waker(waker);    
    let mut p = a.future.take().unwrap();    
    let _ = p.as_mut().poll(&mut cx);
    a.future = Some(p);
}

fn schedule(vect: usize) {
    let s = unsafe { &mut SCHED[vect] };
     
    while s.is_empty() == false {
        let a = s.pop().unwrap();
        call_once(a);           
    }
}

fn main() {
    
    unsafe { QUEUE.init(); }
    
    let mut f = unsafe { func(&mut ACTOR[0]) };
    println!("start {}", print_sizeof(&mut f));
    let s = to_static(&mut f);
    
    unsafe { 
        ACTOR[0].future = Some(Pin::new_unchecked(s)); 
        SCHED[0].init();
    
        call_once(&mut ACTOR[0]);
        
        QUEUE.put(&mut MSG);
        
        schedule(0);
        
        QUEUE.put(&mut MSG);
        
        schedule(0);        
    }
}
