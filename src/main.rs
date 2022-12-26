//
// This is the attempt to implement C-style intrusive lists in Rust.
// Note that this code is mostly unsafe!
//

use std::ptr;
use std::cell::Cell;

//
// Embeddable object contains two pointers to prev/next list intems and also
// payload pointer which points to the object containing this list node. This
// pointer is introduced to avoid pointer arithmetic.
//
struct Node<T> {
    prev: *mut Node<T>,
    next: *mut Node<T>,
    payload: *mut T
}

type List<T> = Node<T>;

macro_rules! node_init {
    ($i:expr, $f:ident) => 
    { 
        $i.$f.payload = &mut $i;
        $i.$f.prev = ptr::null_mut();
        $i.$f.next = ptr::null_mut();
    }
}

impl<T> Node<T> {

    //
    // The constructor should also work for static data.
    //
    const fn new() -> Node<T> {
        Node {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
            payload: ptr::null_mut()
        }
    }

    //
    // List (not node!) initialization. Initially both pointers point to itself.
    //
    fn init(&mut self) -> () {
        self.prev = self;
        self.next = self;
        self.payload = ptr::null_mut();
    }

    //
    // List is not empty when both poiters don't point to itself.
    //
    fn is_not_empty(&self) -> bool {
        self.next as *const Node<T> != self
    }

    //
    // Returns reference to object corresponding to list head.
    //
    unsafe fn head(&self) -> Option<&mut T> {
        if self.is_not_empty() { 
            Some(&mut *((*self.next).payload))
        } else { 
            None 
        }
    }

    //
    // Returns reference to object corresponding to list tail.
    //
    unsafe fn tail(&self) -> Option<&mut T> {
        if self.is_not_empty() { 
            Some(&mut *((*self.prev).payload))
        } else {
            None
        }
    }

    //
    // Inserts the specified node in the head of the list.
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
    // Removes the specified node from the list. Note that the list is not 
    // specified, it extracts node from any list.
    //
    fn remove(node: &mut Node<T>) -> () {

        assert!(node.prev != ptr::null_mut());
        assert!(node.next != ptr::null_mut());

        unsafe {
            (*node.prev).next = node.next;
            (*node.next).prev = node.prev;
        }

        node.next = ptr::null_mut();
        node.prev = ptr::null_mut();
    }

    //
    // Removes nodes for which the closure returns true.
    // The function may also be used to traverse the list when the closure  
    // always returns false (each node will be visited and the list will remain
    // unchanged).
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
    // Inserts the specified node before or after the one for which the closure 
    // returned Some(x).
    //
    unsafe fn insert<F>(&mut self, node: &mut Node<T>, closure: F) -> () 
        where F: Fn(&T) -> Option<bool> {

        let mut item = self.next;
        while item != self {
            if let Some(before) = closure(&(*(*item).payload)) {
                let target = if before { 
                    &mut *((*item).prev) 
                } else { 
                    &mut *item 
                };
                target.insert_head(node);
                break;
            }

            item = (*item).next;
        } 
    }
}


fn main() {

    struct Item {
        foo: usize,
        linkage: Node<Item>
    }

    let mut test_list: List<Item> = List::new();

    let mut item =  [ 
        Item { foo: 1, linkage: Node::new() },
        Item { foo: 2, linkage: Node::new() },
        Item { foo: 3, linkage: Node::new() },
        Item { foo: 4, linkage: Node::new() },
        Item { foo: 5, linkage: Node::new() }
    ];

    let mut test =  [ 
        Item { foo: 6, linkage: Node::new() },
        Item { foo: 7, linkage: Node::new() },
    ];

    unsafe {

        test_list.init();

        assert!(test_list.is_not_empty() == false);

        //
        // Insert items 1..5.
        //
        for i in 0..item.len() {
            node_init!(item[i], linkage);
            test_list.append(&mut item[i].linkage);
        }

        assert!(test_list.is_not_empty() == true);

        //
        // Init two extra items 6 and 7.
        //
        for i in 0..test.len() {
            node_init!(test[i], linkage);
        }

        let num = Cell::new(0);        

        //
        // Count items.
        //
        test_list.filter(|_item| {
            num.set(num.get() + 1);
            false
        });

        assert!(num.get() == item.len());
        assert!(test_list.head().unwrap().foo == 1);
        assert!(test_list.tail().unwrap().foo == 5);

        //
        // Insert hew head (before 1).
        //
        test_list.insert(&mut test[0].linkage, |item| {
            if item.foo == 1 { Some(true) } else { None }
        });

        let tail_val = test_list.tail().unwrap().foo;

        //
        // Insert new tail.
        //
        test_list.insert(&mut test[1].linkage, |item| {
            if item.foo == tail_val { Some(false) } else { None }
        });

        assert!(test_list.head().unwrap().foo == 6);
        assert!(test_list.tail().unwrap().foo == 7);

        test_list.filter(|item| {
            println!("item {}", item.foo);
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
