use crate::{node_traverser::PathNodeTraverser, path_finder::PathFinder};

pub trait PathNavigator {

}

pub struct LandPathNavigator<T: PathNodeTraverser> {
    path_finder: PathFinder<T>
}