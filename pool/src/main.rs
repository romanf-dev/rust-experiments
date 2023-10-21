
struct Block {
    foo: u32
}

struct Pool<'a, T, const N: usize> {
    used: usize,
    slice: Option<&'a mut [T]>
}

macro_rules! pool_new {
    () => { Pool { used: 0, slice: None } }
}

impl<'a, T: Sized, const N: usize> Pool<'a, T, N> {

    fn init<'b: 'a>(&mut self, arr: &'b mut [T; N]) {
        self.slice = Some(&mut arr[0..N]);
    }

    fn alloc<'b>(&mut self) -> Option<&'b mut T> where 'a: 'b {
        if self.used < N {
            let remaining_items = self.slice.take().unwrap();
            let (item, rest) = remaining_items.split_at_mut(1);
            self.used += 1;
            self.slice = Some(rest);
            Some(&mut item[0])
        } else {
            None
        }
    }
}

fn main() {
    
    const BLK: Block = Block { foo: 0 };
    const SZ: usize = 10;

    let mut array: [Block; SZ] = [BLK; SZ];
    let mut pool: Pool<Block, SZ> = pool_new!();    
    
    pool.init(&mut array);
    
    while let Some(blk) = pool.alloc() {
        blk.foo = 1; 
        println!("alloc succeeded");
    }
}
