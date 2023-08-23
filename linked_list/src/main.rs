use core::marker::PhantomData;

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
    
    fn to_obj(&mut self) -> &mut T {
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

    fn get_head(&mut self) -> Option<&mut Node<T>> {
        match self.root.links { 
            Some((_, next)) => 
                if next != &mut self.root { 
                    unsafe { Some(&mut *next) } 
                } else { 
                    None 
                },
            _ => None
        }       
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

    fn pop(&mut self) -> Option<&mut T> {
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

struct Item {
    foo: usize,
    linkage: Node<Self>
}

impl Linkable for Item {
    fn to_links(&mut self) -> &mut Node<Self> {
        &mut self.linkage
    }
}

static mut LIST: List<Item> = List::<Item>::new();

fn main() {
    let mut items: [Item; 3] = [
        Item { foo: 1, linkage: Node::new() },
        Item { foo: 2, linkage: Node::new() },
        Item { foo: 3, linkage: Node::new() },
    ];

    unsafe {
        LIST.init();
        LIST.push(&mut items[0]);
        LIST.push(&mut items[1]);
        LIST.push(&mut items[2]);
                
        while let Some(val) = LIST.pop() {
            println!("val = {}", val.foo);
        }
    }
}
