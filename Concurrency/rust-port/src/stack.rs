use std::collections::LinkedList;
use std::ptr;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

/// `Stack` is a trait for stack, user can push pop in this stack.
pub trait Stack<T> {
    fn push(&self, obj: T);

    fn empty(&self) -> bool;

    fn pop(&self) -> Option<T>;

    fn pop_wait(&self) -> T {
        unimplemented!()
    }
}

macro_rules! impl_sync {
    ($t: ty) => {
        unsafe impl<T> Sync for $t {}
    };
}

#[derive(Default)]
pub struct LockStack<T> {
    pub(crate) guard: Mutex<LinkedList<T>>,
}

impl_sync!(LockStack<T>);

impl<T: Copy> Stack<T> for LockStack<T> {
    fn push(&self, obj: T) {
        let mut guard = self.guard.lock().unwrap();
        guard.push_back(obj);
    }

    fn empty(&self) -> bool {
        self.guard.lock().unwrap().is_empty()
    }

    /// If there is nothing, calling to pop will immediately got None.
    fn pop(&self) -> Option<T> {
        let mut guard = self.guard.lock().unwrap();
        match guard.back() {
            Some(&s) => {
                guard.pop_back();
                Some(s)
            }
            None => None,
        }
    }
}

#[derive(Default)]
pub struct CondStack<T> {
    guard: LockStack<T>,
    cond: Condvar,
}

impl_sync!(CondStack<T>);

impl<T: Copy> Stack<T> for CondStack<T> {
    fn push(&self, obj: T) {
        self.guard.push(obj);
        self.cond.notify_one();
    }

    fn empty(&self) -> bool {
        self.guard.empty()
    }

    fn pop(&self) -> Option<T> {
        self.guard.pop()
    }

    fn pop_wait(&self) -> T {
        let result = self.guard.guard.lock();
        let mut entity: MutexGuard<LinkedList<T>> = result.unwrap();

        if !entity.is_empty() {
            entity.pop_back().unwrap()
        } else {
            let e = self.cond.wait_until(entity, |cfg| !cfg.is_empty());
            entity = e.unwrap();
            entity.pop_back().unwrap()
        }
    }
}

/// This stack is a basic implemention, it will cause memory leak,
/// cause `Node` cannot manage it's memory efficiency.
pub struct AtomicStack<T> {
    head: AtomicPtr<Node<T>>,
}

/// Node is an object saving values.
/// `Node` doesn't manages any
pub struct Node<T> {
    val: T,
    /// Well, maybe I can use NonNull<Option>?
    next: *mut Node<T>,
}

impl<T> Node<T> {
    pub fn new(obj: T) -> Self {
        Node {
            next: null_mut(),
            val: obj,
        }
    }
}

impl<T> Stack<T> for AtomicStack<T> {
    fn push(&self, obj: T) {
        let new_node: NonNull<Node<T>> = Box::into_raw_non_null(Box::new(Node::new(obj)));

        loop {
            let old_head = self.head.load(Ordering::Relaxed);
            unsafe {
                (*(new_node.as_ptr())).next = old_head;
            }
            if self
                .head
                .compare_and_swap(old_head, new_node.as_ptr(), Ordering::Release)
                == old_head
            {
                return;
            }
        }
    }

    fn empty(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        head.is_null()
    }

    fn pop(&self) -> Option<T> {
        loop {
            // take a snapshot
            let head = self.head.load(Ordering::Acquire);
            if head.is_null() {
                return None;
            } else {
                let next = unsafe { (*head).next };

                // if snapshot is still good, update from `head` to `next`
                if self.head.compare_and_swap(head, next, Ordering::Release) == head {
                    // extract out the data from the now-unlinked node
                    // **NOTE**: leaks the node!
                    return Some(unsafe { ptr::read(&(*head).val) });
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::stack::{CondStack, LockStack, Stack};
    use std::sync::Arc;
    use std::thread::spawn;

    fn concurrency_get_test<T: Stack<i32> + Default + Sync + Send + 'static>() {
        let s1 = T::default();
        let cnt_size = 10000;
        for i in 0..cnt_size {
            s1.push(i);
        }

        let arc = Arc::new(s1);
        let mut tasks = vec![];
        for _ in 0..5 {
            let stack = arc.clone();
            tasks.push(spawn(move || {
                let mut cnt = 0;
                while !stack.empty() {
                    if stack.pop().is_some() {
                        cnt += 1;
                    }
                }
                cnt
            }));
        }

        assert_eq!(cnt_size, tasks.into_iter().map(|t| t.join().unwrap()).sum());
    }

    #[test]
    fn test_stack_1() {
        concurrency_get_test::<LockStack<i32>>();
    }

    #[test]
    fn test_stack_2() {
        concurrency_get_test::<CondStack<i32>>();
    }

    #[test]
    fn test_cond_wait() {
        let cond = CondStack::default();
        let mut task_vec = vec![];
        let arc_cond = Arc::new(cond);
        let task_cnt = 10;
        for _ in 0..task_cnt {
            let cond_t = arc_cond.clone();
            task_vec.push(spawn(move || cond_t.pop_wait()));
        }

        for i in 0..task_cnt {
            arc_cond.push(i);
        }

        let mut v: Vec<i32> = task_vec.into_iter().map(|t| t.join().unwrap()).collect();
        v.sort();

        assert_eq!(v.len() as i32, task_cnt);
        for ta in 0..task_cnt {
            assert_eq!(ta, v[ta as usize]);
        }
    }
}
