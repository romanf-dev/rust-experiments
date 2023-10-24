use core::marker::PhantomData;
use core::cell::Cell;
use core::ops::DerefMut;
use core::ops::Deref;

struct RefOwner<'a, T> {
    inner: Option<&'a mut T>
}

impl<'a, T> RefOwner<'a, T> {
    fn new(r: &'a mut T) -> Self {
        Self {
            inner: Some(r)
        }
    }
}

impl<'a, T> Deref for RefOwner<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        assert!(self.inner.is_some());
        self.inner.as_deref().unwrap()
    }
}

impl<'a, T> DerefMut for RefOwner<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        assert!(self.inner.is_some());
        self.inner.as_deref_mut().unwrap()
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

    fn to_obj<'a>(&self) -> RefOwner<'a, T> {
        let ptr = self.payload.take().unwrap();
        unsafe { RefOwner::new(&mut *ptr) }
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
    
    fn enqueue(&self, mut wrapper: RefOwner<'a, T>) {
        let object = wrapper.inner.take().unwrap();
        let ptr: *mut T = object;
        let node = object.to_links();
        let (prev, next) = self.root.links.take().unwrap();
        node.links.set(Some((prev, &self.root as *const Node<T>)));
        node.payload.set(Some(ptr));
        self.root.links.set(Some((node, next)));
        unsafe { (*prev).set_next(node); }
    }

    fn dequeue(&self) -> Option<RefOwner<'a, T>> {
        if let Some(node) = self.peek_head_node() {
            node.unlink();
            Some(node.to_obj())
        } else {
            None
        }
    }
}

struct Item {
    foo: usize,
    linkage: Node<Self>
}

impl Linkable for Item {
    fn to_links(&self) -> &Node<Self> {
        &self.linkage
    }
}
    
fn main() {

    let list: List<Item> = List::<Item>::new();

    let mut item0: Item = Item { foo: 1, linkage: Node::new() };
    let mut item1: Item = Item { foo: 2, linkage: Node::new() };
    let mut item2: Item = Item { foo: 3, linkage: Node::new() };
    let mut item3: Item = Item { foo: 4, linkage: Node::new() };

    let i0 = RefOwner::new(&mut item0);
    let i1 = RefOwner::new(&mut item1);
    let i2 = RefOwner::new(&mut item2);
    let i3 = RefOwner::new(&mut item3);
    
    list.init();
    list.enqueue(i0);
    list.enqueue(i1);
    list.enqueue(i2);
    list.enqueue(i3);
    
    assert!(list.is_empty() == false);
    
    while let Some(val) = list.dequeue() {
        println!("val = {}", val.foo);
    }
}
