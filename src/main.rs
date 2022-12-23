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
    ($i:expr, $f:ident) => { $i.$f.payload = &mut $i }
}

impl<T> Node<T> {

    //
    // Since this constructor should also work for static data it must be 'const'.
    //
    const fn new() -> Node<T> {
        Node {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
            payload: ptr::null_mut()
        }
    }

    //
    // List (not node!) initialization.
    // Initially both pointers point to itself.
    //
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

    //
    // List is not empty when both poiters don't point to itself.
    //
    fn is_not_empty(&self) -> bool {
        self.next as *const Node<T> != self
    }

    //
    // Peeks list's head item or returns None.
    //
    unsafe fn head(&self) -> Option<&mut T> {
        if self.is_not_empty() { 
            Some(&mut (*((*self.next).payload))) 
        } else { 
            None 
        }
    }

    //
    // Peeks list's tail or returns None.
    //
    unsafe fn tail(&self) -> Option<&mut T> {
        if self.is_not_empty() { 
            Some(&mut (*((*self.prev).payload)))
        } else {
            None
        }
    }

    //
    // Inserts specified node at the head of the list.
    //
    fn insert_head(&mut self, node: &mut Node<T>) -> () {
        
        assert!(node.prev == ptr::null_mut());
        assert!(node.next == ptr::null_mut());

        node.next = self.next;
        node.prev = self;

        unsafe {
            (*self.next).prev = node;
        }

        self.next = node;
    }

    //
    // Appends the node to the tail.
    //
    unsafe fn append(&mut self, node: &mut Node<T>) -> () {
        (*self.prev).insert_head(node);
    }

    //
    // Removes the specified node from the list. Note that 
    //
    fn remove(node: &mut Node<T>) -> () {
        unsafe {
            (*node.prev).next = node.next;
            (*node.next).prev = node.prev;
        }

        node.next = ptr::null_mut();
        node.prev = ptr::null_mut();
    }

    //
    // Removes nodes for which the closure returns true.
    // The function may lso be used for list traversal when the closure always returns false.
    //
    unsafe fn filter<F>(&mut self, closure: F) -> () where F: Fn(&T) -> bool {
        let mut item = self.next;

        while item != self {
            let del = closure(&(*(*item).payload));
            item = (*item).next;
            
            if del {
                Node::<T>::remove(&mut *(*item).prev);
            }
        } 
    }

    //
    // Inserts the specified node before of after the one for which the closure returned Some(x).
    //
    unsafe fn insert<F>(&mut self, node: &mut Node<T>, closure: F) -> () 
        where F: Fn(&T) -> Option<bool> {
        let mut item = self.next;

        while item != self {
            if let Some(before) = closure(&(*(*item).payload)) {
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

    let mut item =  [ 
        Item { foo: 1, linkage: Node::new() },
        Item { foo: 2, linkage: Node::new() },
        Item { foo: 3, linkage: Node::new() },
        Item { foo: 4, linkage: Node::new() },
        Item { foo: 5, linkage: Node::new() }
    ];

    unsafe {

        test_list.init();

        for i in 0..5 {
            node_init2!(item[i], linkage);

        }

        for i in 0..4 {
            test_list.append(&mut item[i].linkage);
        }

        test_list.filter(|item| {
            println!("before {}", item.foo);
            false
        });


        test_list.insert(&mut item[4].linkage, |item| {
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
