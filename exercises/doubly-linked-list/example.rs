// this module adds some functionality based on the required implementations here
// like: `LinkedList::pop_back`, `Clone` and `IntoIterator` for `LinkedList<T>`
// You are free to use anything in it, but it's primarily for the test framework.
mod pre_implemented;

use std::ptr::NonNull;

type NodePtr<T> = NonNull<Node<T>>;
type OptNodePtr<T> = Option<NodePtr<T>>;

pub struct LinkedList<T> {
    back: OptNodePtr<T>,
    front: OptNodePtr<T>,
    len: usize,
    // The PhantomData signals dropck that we actually own `T`
    // I'm only aware of one case where this actually matters, which is when
    // using the dropck_eyepatch feature in Drop. We aren't using that here, so this is likely
    // unnecessary.
    // It can't hurt however and I'm not entirely certain that dropck_eyepatch is the only
    // case where it matters
    marker: std::marker::PhantomData<Box<T>>,
}

unsafe impl<T: Send> Send for LinkedList<T> {}
unsafe impl<T: Sync> Sync for LinkedList<T> {}

pub struct Cursor<'a, T> {
    list: &'a mut LinkedList<T>,
    node: OptNodePtr<T>,
}

struct Node<T> {
    element: T,
    next: OptNodePtr<T>,
    prev: OptNodePtr<T>,
}

impl<T> Node<T> {
    fn new_linkless(element: T) -> NodePtr<T> {
        Self::allocate(element, None, None)
    }

    fn allocate(element: T, prev: OptNodePtr<T>, next: OptNodePtr<T>) -> NodePtr<T> {
        unsafe {
            NonNull::new_unchecked(Box::into_raw(Box::new(Self {
                element,
                next,
                prev,
            })))
        }
    }
}

trait NodePtrHelper<T> {
    fn get_next(&mut self) -> &mut OptNodePtr<T>;
    fn get_prev(&mut self) -> &mut OptNodePtr<T>;
    fn link(left: NodePtr<T>, right: NodePtr<T>);
    fn insert_between(self, prev: NodePtr<T>, next: NodePtr<T>) -> NodePtr<T>;
    fn insert_new_after(self, element: T) -> NodePtr<T>;
    fn insert_new_before(self, element: T) -> NodePtr<T>;
    fn unlink_next(&mut self) -> OptNodePtr<T>;
    fn unlink_prev(&mut self) -> OptNodePtr<T>;
    fn into_inner(self) -> T;
}

impl<T> NodePtrHelper<T> for NodePtr<T> {
    fn get_next(&mut self) -> &mut OptNodePtr<T> {
        unsafe { &mut self.as_mut().next }
    }

    fn get_prev(&mut self) -> &mut OptNodePtr<T> {
        unsafe { &mut self.as_mut().prev }
    }

    fn link(mut left: NodePtr<T>, mut right: NodePtr<T>) {
        *left.get_next() = Some(right);
        *right.get_prev() = Some(left);
    }

    fn insert_between(self, prev: NodePtr<T>, next: NodePtr<T>) -> Self {
        Self::link(prev, self);
        Self::link(self, next);
        self
    }

    fn insert_new_after(mut self, element: T) -> Self {
        if let Some(next) = *self.get_next() {
            Node::new_linkless(element)
                .insert_between(self, next)
        } else {
            let new_node = Node::new_linkless(element);
            NodePtr::link(self, new_node);
            new_node
        }
    }

    fn insert_new_before(mut self, element: T) -> Self {
        if let Some(prev) = *self.get_prev() {
            Node::new_linkless(element)
                .insert_between(prev, self)
        } else {
            let new_node = Node::new_linkless(element);
            NodePtr::link(new_node, self);
            new_node
        }
    }

    // returns next of self, if it exists
    // leaves self untouched
    fn unlink_next(&mut self) -> OptNodePtr<T> {
        self.get_next().map(|mut next| {
            *next.get_prev() = None;
            next
        })
    }

    // see unlink_next
    fn unlink_prev(&mut self) -> OptNodePtr<T> {
        self.get_prev().map(|mut prev| {
            *prev.get_next() = None;
            prev
        })
    }

    // must not be linked to from other pointers
    // own links are irrelevant
    fn into_inner(self) -> T {
        unsafe {
            Box::from_raw(self.as_ptr()).element
        }
    }
}

impl<T> LinkedList<T> {
    pub fn new() -> Self {
        LinkedList {
            back: None,
            front: None,
            len: 0,
            marker: std::marker::PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn cursor_front(&mut self) -> Cursor<T> {
        Cursor {
            node: self.front,
            list: self,
        }
    }

    pub fn cursor_back(&mut self) -> Cursor<T> {
        Cursor {
            node: self.back,
            list: self,
        }
    }

    pub fn iter(&self) -> Iter<T> {
        Iter {
            next_node: self.front,
            marker: std::marker::PhantomData,
        }
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut cursor = self.cursor_front();
        while let Some(_) = cursor.take() {}
    }
}

pub struct Iter<'a, T> {
    next_node: OptNodePtr<T>,
    marker: std::marker::PhantomData<&'a LinkedList<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let node_ptr = self.next_node?;
        unsafe {
            let current_node = &*node_ptr.as_ptr();
            self.next_node = current_node.next;
            Some(&current_node.element)
        }
    }
}

impl<T> Cursor<'_, T> {
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        unsafe {
            self.node.map(|node| &mut (*node.as_ptr()).element)
        }
    }

    pub fn next(&mut self) -> Option<&mut T> {
        self._step(NodePtr::get_next)
    }

    pub fn prev(&mut self) -> Option<&mut T> {
        self._step(NodePtr::get_prev)
    }

    fn _step(
        &mut self,
        get_next: impl Fn(&mut NodePtr<T>) -> &mut OptNodePtr<T>
    ) -> Option<&mut T> {
        unsafe {
            if let Some(new_pos) = *get_next(self.node.as_mut()?) {
                self.node = Some(new_pos);
                Some(&mut (*new_pos.as_ptr()).element)
            } else {
                None
            }
        }
    }

    pub fn take(&mut self) -> Option<T> {
        let mut node = self.node?;
        let next = node.unlink_next();
        let prev = node.unlink_prev();

        match (prev, next) {
            (Some(prev), Some(next)) => {
                self.node = Some(next);
                NodePtr::link(prev, next);
            },
            (Some(_), None) => {
                self.node = prev;
                self.list.back = prev;
            },
            (None, Some(_)) => {
                self.node = next;
                self.list.front = next;
            },
            _ => {
                self.node = None;
                self.list.back = None;
                self.list.front = None;
            },
        };
        self.list.len -= 1;
        Some(node.into_inner())
    }

    pub fn insert_after(&mut self, element: T) {
        self._insert(element, |list, cursor_node, element| {
            let new_node = cursor_node.insert_new_after(element);
            if list.back == Some(cursor_node) {
                list.back = Some(new_node);
            }
        });
    }

    pub fn insert_before(&mut self, element: T) {
        self._insert(element, |list, cursor_node, element| {
            let new_node = cursor_node.insert_new_before(element);
            if list.front == Some(cursor_node) {
                list.front = Some(new_node);
            }
        });
    }

    // put into list, if empty, else do whatever callback says
    fn _insert(&mut self, element: T, callback: impl Fn(&mut LinkedList<T>, NodePtr<T>, T)) {
        let cursor_node = match self.node {
            Some(node) => node,
            None => { // list empty
                self.node = Some(Node::new_linkless(element));
                self.list.back = self.node;
                self.list.front = self.node;
                self.list.len += 1;
                return
            }
        };
        callback(&mut self.list, cursor_node, element);
        self.list.len += 1;
    }
}
