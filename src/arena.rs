
#[derive(Debug)]
pub struct Arena<T> {
    arena: Vec<T>,
}

impl<T> Arena<T>
    where
        T: Default + Sized {
    pub fn with_capacity(initial_capacity: usize) -> Arena<T> {
        Arena {
            arena: Vec::with_capacity(initial_capacity),
        }
    }

    pub fn insert<Indexer>(&mut self, entry: T, indexer: Indexer) -> (usize, &mut T)
        where
            Indexer: Fn(&T) -> usize {
        let index = indexer(&entry);
        self.arena.insert(index, entry);
        (index, self.arena.get_mut(index).unwrap())
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.arena.get(index)
    }
}

pub struct ChildrenIterator<'a, T> {
    arena: &'a Arena<T>,
    children: &'a Vec<u64>,
    index: usize,
}

impl<'a, T> ChildrenIterator<'a, T> {
    pub fn new(arena: &'a Arena<T>, children: &'a Vec<u64>) -> ChildrenIterator<'a, T> {
        ChildrenIterator {
            arena,
            children,
            index: 0,
        }
    }
}

impl<'a, T> Iterator for ChildrenIterator<'a, T>
    where
        T: Default + Sized {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        let child_ino_opt = self.children.get(self.index);
        self.index += 1;
        match child_ino_opt {
            None => None,
            Some(child_ino) => {
                // TODO ino_to_arena_index
                let arena_index = (child_ino - 1) as usize;
                self.arena.get(arena_index)
            },
        }
    }
}