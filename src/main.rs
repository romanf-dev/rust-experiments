use std::ptr;

struct Node<T> {
    prev: *mut Node<T>,
    next: *mut Node<T>,
    payload: *mut T
}

type List<T> = Node<T>;

/*
macro_rules! node_init {
    ($i:ident, $f:ident) => { $i.$f.node_init(&mut $i) }
}
*/

macro_rules! node_init2 {
    ($i:ident, $f:ident) => { $i.$f.payload = &mut $i }
}

impl<T> Node<T> {

    const fn new() -> Node<T> {
        Node {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
            payload: ptr::null_mut()
        }
    }

    fn init(&mut self) -> () {
        self.prev = self;
        self.next = self;
        self.payload = ptr::null_mut();
    }

/*
    fn node_init(&mut self, obj: &mut T) -> () {
        self.prev = ptr::null_mut();
        self.next = ptr::null_mut();
        self.payload = obj;
    }
*/

    fn is_not_empty(&self) -> bool {
        self.next as *const Node<T> != self
    }

    unsafe fn head(&self) -> Option<&mut T> {
        if self.is_not_empty() { 
            Some(&mut (*((*self.next).payload))) 
        } else { 
            None 
        }
    }

    unsafe fn tail(&self) -> Option<&mut T> {
        if self.is_not_empty() { 
            Some(&mut (*((*self.prev).payload)))
        } else {
            None
        }
    }

    fn insert_head(&mut self, node: &mut Node<T>) -> () {
        node.next = self.next;
        node.prev = self;

        unsafe {
            (*self.next).prev = node;
        }

        self.next = node;
    }

    unsafe fn append(&mut self, node: &mut Node<T>) -> () {
        (*self.prev).insert_head(node);
    }

    fn remove(node: &mut Node<T>) -> () {
        unsafe {
            (*node.prev).next = node.next;
            (*node.next).prev = node.prev;
        }

        node.next = ptr::null_mut();
        node.prev = ptr::null_mut();
    }

    unsafe fn filter<F>(&mut self, f: F) -> () where F: Fn(&T) -> bool {
        let mut item = self.next;

        while item != self {
            let del = f(&(*(*item).payload));
            item = (*item).next;
            
            if del {
                Node::<T>::remove(&mut *(*item).prev);
            }
        } 
    }

    unsafe fn insert<F>(&mut self, node: &mut Node<T>, f: F) -> () where F: Fn(&T) -> Option<bool> {
        let mut item = self.next;

        while item != self {
            if let Some(before) = f(&(*(*item).payload)) {
            let target = if before { &mut *((*item).prev) } else { &mut *item };
                target.insert_head(node);
                break;
            }

            item = (*item).next;
        } 
    }
}

struct Item {
    foo: usize,
    linkage: Node<Item>
}

fn main() {
    let mut test_list: List<Item> = List::new();

    let mut item1: Item = Item { foo: 1, linkage: Node::new() };
    let mut item2: Item = Item { foo: 2, linkage: Node::new() };
    let mut item3: Item = Item { foo: 3, linkage: Node::new() };


    unsafe {

        test_list.init();

        node_init2!(item1, linkage);
        node_init2!(item2, linkage);
        node_init2!(item3, linkage);

        test_list.append(&mut item1.linkage);
        test_list.append(&mut item2.linkage);


        test_list.filter(|item| {
            println!("before {}", item.foo);
            false
        });


        test_list.insert(&mut item3.linkage, |item| {
            if item.foo == 2 { Some(false) } else { None }
        });

        test_list.filter(|item| {
            println!("after {}", item.foo);
            false
        });

        if let Some(x) = test_list.tail() {
            println!("tail {}", x.foo);
        }

        if let Some(x) = test_list.head() {
            println!("head {}", x.foo);
        }
    }
}
