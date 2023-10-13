use core::marker::PhantomData;
use core::cell::Cell;

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
    
    fn to_obj<'a>(&self) -> &'a mut T {
        let ptr = self.payload.take().unwrap();
        unsafe { &mut *ptr }
    }
}

trait Linkable: Sized {
    fn to_links(&self) -> &Node<Self>;
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
    
    fn enqueue<'b: 'a>(&self, object: &'b mut T) {
        let ptr: *mut T = object;
        let node = object.to_links();
        let (prev, next) = self.root.links.take().unwrap();
        node.links.set(Some((prev, &self.root as *const Node<T>)));
        node.payload.set(Some(ptr));
        self.root.links.set(Some((node, next)));
        unsafe { (*prev).set_next(node); }
    }

    fn dequeue<'b: 'a>(&self) -> Option<&'b mut T> {
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

struct Item {
    foo: usize,
    linkage: Node<Self>
}

impl Linkable for Item {
    fn to_links(&self) -> &Node<Self> {
        &self.linkage
    }
}

static mut LIST: List<Item> = List::<Item>::new();

fn main() {
    let mut items: [Item; 4] = [
        Item { foo: 1, linkage: Node::new() },
        Item { foo: 2, linkage: Node::new() },
        Item { foo: 3, linkage: Node::new() },
        Item { foo: 4, linkage: Node::new() },
    ];

    unsafe {
        LIST.init();
        LIST.enqueue(&mut items[0]);
        LIST.enqueue(&mut items[1]);
        LIST.enqueue(&mut items[2]);
        LIST.enqueue(&mut items[3]);
        
        assert!(LIST.is_empty() == false);
        
        while let Some(val) = LIST.dequeue() {
            println!("val = {}", val.foo);
        }
    }
}
