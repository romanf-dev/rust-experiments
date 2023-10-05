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
    
    fn to_obj(&self) -> &mut T {
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

    fn get_head(&self) -> Option<&Node<T>> {
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
    
    fn push<'b>(&self, object: &'b mut T) {
        let ptr: *mut T = object;
        let node = object.to_links();
        let (prev, next) = self.root.links.take().unwrap();
        node.links.set(Some((prev, &self.root as *const Node<T>)));
        self.root.links.set(Some((node, next)));
        unsafe { (*prev).set_next(node); }
        node.payload.set(Some(ptr));
    }

    fn pop(&self) -> Option<&mut T> {
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
        LIST.push(&mut items[0]);
        LIST.push(&mut items[1]);
        LIST.push(&mut items[2]);
        LIST.push(&mut items[3]);
                
        while let Some(val) = LIST.pop() {
            println!("val = {}", val.foo);
        }
    }
}
