use std::collections::VecDeque;
use arboretum_td::{exact::TamakiPid, graph::{HashMapGraph, MutableGraph as ArboretumMutableGraph}, solver::{AtomSolver, ComputationResult}};
use crabnets::{*, attributes::*, locales::*};
use itertools::Itertools;
use crate::{switch_selection_instance::SwitchSelectionGraph, solver::errors::GraphError};





#[derive(Clone, Default)]
pub struct Bag {
    pub vertices: Vec<usize>,
}

// Bag::AttributeCollection
impl AttributeCollection for Bag {
    fn new() -> Self {
        Bag { vertices: Vec::new() }
    }
}



#[derive(Clone, Default)]
pub struct TreeDecomposition {
    graph: graph!(A ---X--> A with VertexAttributeCollectionType = Bag),
    pub max_bag_size: usize,
    pub root_id: usize,
}

// TreeDecomposition::TreeDecomposition
impl TreeDecomposition {
    pub fn dfs_postordering(&self) -> Vec<usize> {
        let mut answer: Vec<usize> = Vec::with_capacity(self.count_v());
        let mut bag_stack: VecDeque<usize> = VecDeque::from([self.root_id]);
        while !bag_stack.is_empty() {
            let last_bag = *bag_stack.back().unwrap();
            if self.v_degree_out(&last_bag).unwrap() > 0 && (answer.len() == 0 || !self.iter_adjacent_out(&last_bag).unwrap().contains(answer.last().unwrap())) {
                bag_stack.extend(self.iter_adjacent_out(&last_bag).unwrap());
                continue;
            }
            answer.push(bag_stack.pop_back().unwrap());
        }
        answer
    }

    pub fn for_switch_selection_graph(graph: &SwitchSelectionGraph) -> Result<TreeDecomposition, GraphError> {
        let mut arboretum_graph = HashMapGraph::new();
        for id in graph.iter_v() {
            arboretum_graph.add_vertex(id);
        }
        for edge in graph.iter_e() {
            arboretum_graph.add_edge(edge.id1, edge.id2);
        }
        match TamakiPid::with_graph(&arboretum_graph).compute() {
            ComputationResult::ComputedTreeDecomposition(td) => {
                let mut answer: graph!(A ---X--> A with VertexAttributeCollectionType = Bag) = Graph::new();
                for bag in td.bags() {
                    let vertex_set = Vec::from_iter(bag.vertex_set.iter().sorted().cloned());
                    answer.add_v(Some(bag.id));
                    answer.v_attrs_mut(&bag.id).unwrap().vertices = vertex_set;
                }
                let tree_decomposition_root_id = answer.iter_v().next().unwrap();
                let mut unvisited_bags = VecDeque::from([tree_decomposition_root_id]);
                while !unvisited_bags.is_empty() {
                    let curr_bag_id = unvisited_bags.pop_front().unwrap();
                    for &adjacent_bag_id in td.bags[curr_bag_id].neighbors.iter() {
                        if answer.contains_e(&curr_bag_id, &adjacent_bag_id, &0).is_none() {
                            answer.add_e(&curr_bag_id, &adjacent_bag_id, true, None).unwrap();
                            unvisited_bags.push_back(adjacent_bag_id);
                        }
                    }
                }
                Ok(TreeDecomposition { graph: answer, max_bag_size: td.max_bag_size, root_id: tree_decomposition_root_id })
            },
            ComputationResult::Bounds(bounds) => {
                if bounds.lowerbound == graph.count_v() - 1 {
                    let mut answer: graph!(A ---X--> A with VertexAttributeCollectionType = Bag) = Graph::new();
                    answer.add_v(Some(0));
                    answer.v_attrs_mut(&0).unwrap().vertices = graph.iter_v().sorted().collect();
                    Ok(TreeDecomposition { graph: answer, max_bag_size: graph.count_v(), root_id: 0 })
                } else {
                    Err(GraphError::from_str("Failed to compute a tree decomposition."))
                }
            },
        }
    }
}

// TreeDecomposition::ImmutableGraphContainer
impl ImmutableGraphContainer for TreeDecomposition {
    type EdgeAttributeCollectionType = ();
    type EdgeIdType = u8;
    type LocaleType = SimpleDirectedLocale<(), Bag, usize>;
    type VertexAttributeCollectionType = Bag;
    type VertexIdType = usize;

    fn unwrap(&self) -> &Graph<Self::EdgeAttributeCollectionType, Self::EdgeIdType, Self::LocaleType, Self::VertexAttributeCollectionType, Self::VertexIdType> {
        &self.graph
    }
}
