use super::{PlayerService, Player};

// Class that defines utility methods on player
// for easily interacting with flag-based metadata values
// eg. is_on_fire and set_on_fire

macro_rules! flag_impl {
    ($flag:tt, $name:tt, $value:tt) => {
        paste::paste!(
            pub fn [<is_ $name>](&self) -> bool {
                self.metadata.$flag & $value != 0
            }
        
            pub fn [<set_ $name>](&mut self, value: bool) {
                if value {
                    self.metadata.[<set_ $flag>](self.metadata.$flag | $value);
                } else {
                    self.metadata.[<set_ $flag>](self.metadata.$flag & !$value);
                }
            }
        );     
    };
}

impl<P: PlayerService> Player<P> {
    flag_impl!(shared_flags, on_fire, 1);
    flag_impl!(shared_flags, shift_key_down, 2);
    flag_impl!(shared_flags, sprinting, 8);
    flag_impl!(shared_flags, swimming, 16);
    flag_impl!(shared_flags, invisible, 32);
    flag_impl!(shared_flags, glowing, 64);
    flag_impl!(shared_flags, fall_flying, 128);

    flag_impl!(living_entity_flags, using_item, 1);
    flag_impl!(living_entity_flags, using_offhand, 2);
    flag_impl!(living_entity_flags, spin_attacking, 4);
}