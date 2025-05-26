use graphite_mc_protocol::types::ItemStack;

#[allow(unused)]
pub trait ItemHolder: Sized {
    fn get_item_stack(&self) -> ItemStack;

    fn visual_count(&self) -> i32 {
        self.get_item_stack().visual_count()
    }

    fn clone_empty(&self) -> Self;

    fn transfer(&mut self, into: &mut Self, max: i32) -> i32 {
        0
    }

    fn transfer_count(from: &mut i32, to: &mut i32, max_size: i32, transfer_limit: i32) -> i32 {
        let transferrable = max_size.saturating_sub(*to);
        let amount = transferrable.min(*from).min(transfer_limit);

        if amount > 0 {
            *to += amount;
            *from -= amount;
            amount
        } else {
            0
        }
    }

    fn transfer_count_i64(from: &mut i64, to: &mut i64, max_size: i64, transfer_limit: i64) -> i64 {
        let transferrable = max_size.saturating_sub(*to);
        let amount = transferrable.min(*from).min(transfer_limit);

        if amount > 0 {
            *to += amount;
            *from -= amount;
            amount
        } else {
            0
        }
    }
}

pub trait ItemHolderRef<'a, H: ItemHolder>: Sized {
    fn get_item_stack(&self) -> ItemStack;

    fn visual_count(&self) -> i32 {
        self.get_item_stack().visual_count()
    }
}

impl <'a, H: ItemHolder> ItemHolderRef<'a, H> for &'a H {
    fn get_item_stack(&self) -> ItemStack {
        H::get_item_stack(self)
    }
}

pub trait ItemHolderRefMut<'a, H: ItemHolder>: Sized {
    fn get_item_stack(&self) -> ItemStack;

    fn visual_count(&self) -> i32 {
        self.get_item_stack().visual_count()
    }

    fn transfer(&mut self, into: &mut Self, max: i32) -> i32;
}

impl <'a, H: ItemHolder> ItemHolderRefMut<'a, H> for &'a mut H {
    fn get_item_stack(&self) -> ItemStack {
        H::get_item_stack(self)
    }

    fn transfer(&mut self, into: &mut Self, max: i32) -> i32 {
        H::transfer(self, into, max)
    }
}

impl ItemHolder for ItemStack {
    fn get_item_stack(&self) -> ItemStack {
        self.clone()
    }

    fn clone_empty(&self) -> Self {
        Self::EMPTY
    }

    fn transfer(&mut self, into: &mut Self, max: i32) -> i32 {
        if into.is_empty() {
            let amount = self.count.min(max);
            if amount > 0 {
                *into = self.clone();
                into.count = amount;
                self.count -= amount;
    
                if self.is_empty() {
                    *self = ItemStack::EMPTY;
                }
                amount
            } else {
                0
            }
        } else if !self.equals_ignore_count(into) {
            0
        } else {
            let max_stack_size = into.get_max_stack_size() as i32;
            let transferred = Self::transfer_count(&mut self.count, &mut into.count, max_stack_size, max);

            if self.is_empty() {
                *self = ItemStack::EMPTY;
            }
            transferred
        }

    }
}