use std::{collections::{HashMap, HashSet, VecDeque}, sync::{mpsc::{self, Receiver, Sender}, Arc, Mutex}, thread::{self, JoinHandle}};
use crabnets::{BasicImmutableGraph, BasicMutableGraph, ImmutableGraphContainer};
use itertools::Itertools;
use crate::{switch_selection_instance::{SwitchSelectionInstance, SwitchSelectionGraph}, tree_decomposition::TreeDecomposition};
use super::{base_solver::*, errors::SolverError};





#[derive(Clone)]
struct TapsMemo {
    pub primary_substations: Vec<usize>,
    table: HashMap<Vec<TapValue>, TapValue>,
}

// TapsMemo::TapsMemo
impl TapsMemo {
    pub fn complete(primary_substations: Vec<usize>) -> TapsMemo {
        TapsMemo {
            primary_substations: primary_substations.clone(),
            table: HashMap::from_iter(
                primary_substations
                .iter()
                .map(|_| (-10..=10i8))
                .multi_cartesian_product()
                .map(|x|
                    (x.clone(), x.iter().map(|y| y.abs()).max().unwrap())
                )
            )
        }
    }

    #[inline]
    pub fn empty(primary_substations: Vec<usize>) -> TapsMemo {
        TapsMemo { primary_substations: primary_substations.clone(), table: HashMap::with_capacity(21usize.pow(primary_substations.len() as u32)) }
    }

    pub fn intersect(&mut self, other: &TapsMemo) {
        let common_primary_substations_self_indices = self.primary_substations
            .iter()
            .enumerate()
            .filter(|&(_, x)| other.primary_substations.binary_search(x).is_ok())
            .map(|(x, _)| x)
            .collect_vec();
        let common_primary_substations_other_indices = common_primary_substations_self_indices
            .iter()
            .map(|&x| other.primary_substations.binary_search(&self.primary_substations[x]))
            .filter(|x| x.is_ok())
            .map(|x| x.unwrap())
            .collect_vec();
        // Decide what to keep and what to remove
        // We only keep entries that have at least one corresponding  entry  in
        // other.table.
        // Among all the corresponding entries, we choose one with  the  lowest
        // value of the objective function and then choose the maximum  between
        // the value stored in that entry and the value  stored  in  the  entry
        // from self.table.
        let mut optimal_corresponding_entries = HashMap::new();
        for (other_taps_positions, &other_obj_value) in other.table.iter() {
            let other_taps_positions_for_common_primary_substations = common_primary_substations_other_indices
                .iter()
                .map(|&x| other_taps_positions[x])
                .collect_vec();
            match optimal_corresponding_entries.get_mut(&other_taps_positions_for_common_primary_substations) {
                Some(value) => if other_obj_value < *value {
                    *value = other_obj_value;
                },
                None => { optimal_corresponding_entries.insert(other_taps_positions_for_common_primary_substations, other_obj_value); },
            }
        }
        let mut entries_to_be_removed = Vec::new();
        for (taps_positions, obj_value) in self.table.iter_mut() {
            let taps_positions_for_common_primary_substations = common_primary_substations_self_indices
                .iter()
                .map(|&x| taps_positions[x])
                .collect_vec();
            match optimal_corresponding_entries.get(&taps_positions_for_common_primary_substations) {
                Some(&value) => if *obj_value < value {
                    *obj_value = value;
                },
                None => entries_to_be_removed.push(taps_positions.clone()),
            }
        }
        for taps_position in entries_to_be_removed.drain(..) {
            self.table.remove(&taps_position);
        }
    }
}



fn locally_feasible_taps_positions(input: Arc<SwitchSelectionInstance>, bag: &Vec<usize>) -> TapsMemo {
    // Possible values of the square voltage at a primary substation.
    // Tap positions are: T = {-10, ..., 10}.
    // Base voltage: B = {1 + 0.1 * t | t \in T}.
    // Square base voltage: {u² | u \in B}; for each t \in T the  squared  base
    // voltage corresponding to t is BASE_VOLTAGE_SQ[t + 10].
    const BASE_VOLTAGE_SQ: [f64; 21] = [0.81, 0.8281, 0.8464, 0.8649, 0.8836, 0.9025, 0.9216, 0.9409, 0.9604, 0.9801, 1.0,
                                        1.0201, 1.0404, 1.0609, 1.0816, 1.1025, 1.1236, 1.1449, 1.1664, 1.1881, 1.21];
    let mut answer = TapsMemo::complete(bag.clone());
    // Consider all possible pairs of primary  substations  from  the  bag.  If
    // there're lines between a pair of the primary  substations,  try  cutting
    // each line in different places and see which tap positions are feasible.
    for left_primary_substation_i in 0..bag.len() {
        let left_primary_substation_id = bag[left_primary_substation_i];
        for right_primary_substation_i in left_primary_substation_i..bag.len() {
            let right_primary_substation_id = bag[right_primary_substation_i];
            for adjacent_id in input.iter_adjacent(&left_primary_substation_id).unwrap() {
                // If the adjacent vertex is  a  secondary  substation  lying  on  a  line  between
                // left_primary_substation_id  and  right_primary_substation_id,  reconstruct   the
                // entire line.
                match input.v_attrs(&adjacent_id).unwrap().line_endpoints {
                    Some(value) => if value != (left_primary_substation_id, right_primary_substation_id) {
                        continue;
                    },
                    None => continue,
                }
                let mut line = Vec::from([left_primary_substation_id, adjacent_id]);
                loop {
                    let last_discovered_substation_id = line.last().unwrap();
                    if input.v_attrs(last_discovered_substation_id).unwrap().tap_position.is_some() {
                        break;
                    }
                    for adjacent_to_last_id in input.iter_adjacent(last_discovered_substation_id).unwrap() {
                        if adjacent_to_last_id != line[line.len() - 2] {
                            line.push(adjacent_to_last_id);
                            break;
                        }
                    }
                }
                // Now that we have a full line, cut all possible edges on  it  one  after  another
                // and compute feasible tap positions for every cut.
                let mut line_memo = TapsMemo::empty(bag.clone());
                for (last_left_substation_i, first_right_substation_i) in (0..(line.len() - 1)).zip(1..line.len()) {
                    let left_line = &line[..=last_left_substation_i];
                    let right_line = &line[first_right_substation_i..];
                    let mut voltage_sq = 1.0;
                    let mut voltage_sq_peak: f64 = 1.0;
                    let mut voltage_sq_gorge: f64 = 1.0;
                    for left_substation_i in 1..left_line.len() {
                        voltage_sq += input.e_attrs(&left_line[left_substation_i - 1], &left_line[left_substation_i], &0).unwrap().x
                                    * left_line[left_substation_i..].iter().map(|x| input.v_attrs(x).unwrap().q).sum::<f64>()
                                    - input.e_attrs(&left_line[left_substation_i - 1], &left_line[left_substation_i], &0).unwrap().r
                                    * left_line[left_substation_i..].iter().map(|x| input.v_attrs(x).unwrap().p).sum::<f64>();
                        voltage_sq_peak = voltage_sq_peak.max(voltage_sq);
                        voltage_sq_gorge = voltage_sq_gorge.min(voltage_sq);
                    }
                    if voltage_sq_peak - voltage_sq_gorge > 1.21 - 0.81 {
                        continue;
                    }
                    let left_tap_position_min = BASE_VOLTAGE_SQ.iter().enumerate().filter(|&(_, &x)| x >= 1.81 - voltage_sq_gorge).next().unwrap().0 as TapValue - 10;
                    let left_tap_position_max = BASE_VOLTAGE_SQ.iter().enumerate().rev().filter(|&(_, &x)| x <= 2.21 - voltage_sq_peak).next().unwrap().0 as TapValue - 10;
                    voltage_sq = 1.0;
                    voltage_sq_peak = 1.0;
                    voltage_sq_gorge = 1.0;
                    for right_substation_i in (0..(right_line.len() - 1)).rev() {
                        voltage_sq += input.e_attrs(&right_line[right_substation_i + 1], &right_line[right_substation_i], &0).unwrap().x
                                    * right_line[..=right_substation_i].iter().map(|x| input.v_attrs(x).unwrap().q).sum::<f64>()
                                    - input.e_attrs(&right_line[right_substation_i + 1], &right_line[right_substation_i], &0).unwrap().r
                                    * right_line[..=right_substation_i].iter().map(|x| input.v_attrs(x).unwrap().p).sum::<f64>();
                        voltage_sq_peak = voltage_sq_peak.max(voltage_sq);
                        voltage_sq_gorge = voltage_sq_gorge.min(voltage_sq);
                    }
                    if voltage_sq_peak - voltage_sq_gorge > 1.21 - 0.81 {
                        continue;
                    }
                    let right_tap_position_min = BASE_VOLTAGE_SQ.iter().enumerate().filter(|&(_, &x)| x >= 1.81 - voltage_sq_gorge).next().unwrap().0 as TapValue - 10;
                    let right_tap_position_max = BASE_VOLTAGE_SQ.iter().enumerate().rev().filter(|&(_, &x)| x <= 2.21 - voltage_sq_peak).next().unwrap().0 as TapValue - 10;
                    // The following operation can take almost 90% of all computation  time!!!  Can
                    // be optimised by, e.g. replacing these memos with DataFrames and using  joins
                    // instead of extending every line_memo with dozens of rows.
                    line_memo.table.extend(answer.table.iter().filter(|&(x, _)|
                        x[left_primary_substation_i] >= left_tap_position_min &&
                        x[left_primary_substation_i] <= left_tap_position_max &&
                        x[right_primary_substation_i] >= right_tap_position_min &&
                        x[right_primary_substation_i] <= right_tap_position_max
                    ).map(|(k, v)| (k.clone(), v.clone())));
                }
                answer = line_memo;
            }
        }
    }
    answer
}

fn thread_workload(input: Arc<SwitchSelectionInstance>, memos: Arc<Mutex<HashMap<usize, TapsMemo>>>, td: Arc<TreeDecomposition>, bag_id: usize, rx: Receiver<usize>) -> Result<(), SolverError> {
    // Create a memo for this bag
    let bag = td.v_attrs(&bag_id).unwrap().vertices.clone();
    let mut memo = locally_feasible_taps_positions(input, &bag);
    // Intersect memo with the memos of the children
    let mut remaining_children: HashSet<usize> = td.iter_adjacent_out(&bag_id).unwrap().collect();
    while !remaining_children.is_empty() {
        let received_bag_id = match rx.recv() {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };
        if remaining_children.contains(&received_bag_id) {
            let child_memo = memos.lock().unwrap()[&received_bag_id].clone();
            memo.intersect(&child_memo);
            remaining_children.remove(&received_bag_id);
        }
    }
    // If memo is empty, the instance is infeasible
    if memo.table.is_empty() {
        return Err(SolverError::from_str("TreeDecompositionSolver. The problem instance is infeasible."));
    }
    // Otherwise, save in the memos collection and send it to the parent
    memos.lock().unwrap().insert(bag_id, memo);
    Ok(())
}

fn solution_graph_setup(solution: &mut SwitchSelectionGraph, dg_kernel: &SwitchSelectionGraph, taps_positions: &HashMap<usize, TapValue>) {
    // Possible values of the square voltage at a primary substation.
    // Tap positions are: T = {-10, ..., 10}.
    // Base voltage: B = {1 + 0.1 * t | t \in T}.
    // Square base voltage: {u² | u \in B}; for each t \in T the  squared  base
    // voltage corresponding to t is BASE_VOLTAGE_SQ[t + 10].
    const BASE_VOLTAGE_SQ: [f64; 21] = [0.81, 0.8281, 0.8464, 0.8649, 0.8836, 0.9025, 0.9216, 0.9409, 0.9604, 0.9801, 1.0,
                                        1.0201, 1.0404, 1.0609, 1.0816, 1.1025, 1.1236, 1.1449, 1.1664, 1.1881, 1.21];
    for (left_primary_substation_id, right_primary_substation_id) in dg_kernel.iter_e().map(|x| (x.id1, x.id2)) {
        for adjacent_id in solution.iter_adjacent(&left_primary_substation_id).unwrap().collect_vec() {
            // If the adjacent vertex is  a  secondary  substation  lying  on  a  line  between
            // left_primary_substation_id  and  right_primary_substation_id,  reconstruct   the
            // entire line.
            match solution.v_attrs(&adjacent_id).unwrap().line_endpoints {
                Some(value) => if value != (left_primary_substation_id, right_primary_substation_id) {
                    continue;
                },
                None => continue,
            }
            let mut line = Vec::from([left_primary_substation_id, adjacent_id]);
            loop {
                let last_discovered_substation_id = line.last().unwrap();
                if solution.v_attrs(last_discovered_substation_id).unwrap().tap_position.is_some() {
                    break;
                }
                for adjacent_to_last_id in solution.iter_adjacent(last_discovered_substation_id).unwrap() {
                    if adjacent_to_last_id != line[line.len() - 2] {
                        line.push(adjacent_to_last_id);
                        break;
                    }
                }
            }
            // Now that we have a full line, cut all possible edges on  it  one  after  another
            // until a feasible cut is found.
            for (last_left_substation_i, first_right_substation_i) in (0..(line.len() - 1)).zip(1..line.len()) {
                let left_line = &line[..=last_left_substation_i];
                let right_line = &line[first_right_substation_i..];
                let mut voltage_sq = BASE_VOLTAGE_SQ[(taps_positions[&left_primary_substation_id] + 10) as usize];
                let mut voltage_sq_peak = voltage_sq;
                let mut voltage_sq_gorge = voltage_sq;
                for left_substation_i in 1..left_line.len() {
                    voltage_sq += solution.e_attrs(&left_line[left_substation_i - 1], &left_line[left_substation_i], &0).unwrap().x
                                * left_line[left_substation_i..].iter().map(|x| solution.v_attrs(x).unwrap().q).sum::<f64>()
                                - solution.e_attrs(&left_line[left_substation_i - 1], &left_line[left_substation_i], &0).unwrap().r
                                * left_line[left_substation_i..].iter().map(|x| solution.v_attrs(x).unwrap().p).sum::<f64>();
                    voltage_sq_peak = voltage_sq_peak.max(voltage_sq);
                    voltage_sq_gorge = voltage_sq_gorge.min(voltage_sq);
                }
                if voltage_sq_peak > 1.21 || voltage_sq_gorge < 0.81 {
                    continue;
                }
                voltage_sq = BASE_VOLTAGE_SQ[(taps_positions[&right_primary_substation_id] + 10) as usize];
                voltage_sq_peak = voltage_sq;
                voltage_sq_gorge = voltage_sq;
                for right_substation_i in (0..(right_line.len() - 1)).rev() {
                    voltage_sq += solution.e_attrs(&right_line[right_substation_i + 1], &right_line[right_substation_i], &0).unwrap().x
                                * right_line[..=right_substation_i].iter().map(|x| solution.v_attrs(x).unwrap().q).sum::<f64>()
                                - solution.e_attrs(&right_line[right_substation_i + 1], &right_line[right_substation_i], &0).unwrap().r
                                * right_line[..=right_substation_i].iter().map(|x| solution.v_attrs(x).unwrap().p).sum::<f64>();
                    voltage_sq_peak = voltage_sq_peak.max(voltage_sq);
                    voltage_sq_gorge = voltage_sq_gorge.min(voltage_sq);
                }
                if voltage_sq_peak > 1.21 || voltage_sq_gorge < 0.81 {
                    continue;
                }
                // If we reach this point, we've found a feasible cut. Record it in the graph.
                solution.e_attrs_mut(&line[last_left_substation_i], &line[first_right_substation_i], &0).unwrap().switch = true;
                break;
            }
        }
    }
    for primary_substation_id in dg_kernel.iter_v() {
        solution.v_attrs_mut(&primary_substation_id).unwrap().tap_position = Some(taps_positions[&primary_substation_id]);
    }
}



struct ThreadMetadata {
    bag_id: usize,
    join_handle: Option<JoinHandle<Result<(), SolverError>>>,
    tx: Option<Sender<usize>>,
}



pub struct TreeDecompositionSolver {
    dg_kernel: SwitchSelectionGraph,
    input: Arc<SwitchSelectionInstance>,
    td: Arc<TreeDecomposition>,
    memos: Option<HashMap<usize, TapsMemo>>,
    thread_count: usize,
}

// TreeDecompositionSolver::BaseSolver
impl BaseSolver for TreeDecompositionSolver {
    fn with_input(input: SwitchSelectionInstance) -> Result<Self, SolverError> {
        let dg_kernel = input.dg_kernel_for_switch_selection();
        let td = match TreeDecomposition::for_switch_selection_graph(&dg_kernel) {
            Ok(value) => value,
            Err(value) => return Err(SolverError::from_string(value.to_string())),
        };
        Ok(TreeDecompositionSolver {
            dg_kernel,
            input: Arc::new(input),
            td: Arc::new(td),
            memos: None,
            thread_count: num_cpus::get() - 1,
        })
    }

    fn get_solution(&self) -> Option<(SwitchSelectionGraph, TapValue)> {
        if self.memos.is_none() {
            return None;
        }
        let memos = self.memos.as_ref().unwrap();
        let mut answer = self.input.as_ref().unwrap().clone();
        // Tap positions will be gradually collected in taps_positions
        let mut taps_positions: HashMap<usize, TapValue> = HashMap::new();
        // Traverse the tree decomposition  top-down  in  breadth-first  search
        // pre-ordering to construct a solution
        let mut bag_queue: VecDeque<usize> = VecDeque::from([self.td.root_id]);
        while !bag_queue.is_empty() {
            // Get the ID of the current bag.
            let curr_bag_id = bag_queue.pop_front().unwrap();
            // Find the best locally feasible solution that  agrees  with  what
            // was already built in taps_positions.
            let curr_entry = memos[&curr_bag_id].table
                .iter()
                .sorted_by_key(|&(_, &x)| x)
                .find(|&(k, _)|
                    k.iter().enumerate().all(|(i, &x)|
                        match taps_positions.get(&memos[&curr_bag_id].primary_substations[i]) {
                            Some(&value) => x == value,
                            None => true,
                        }
                    )
                )
                .unwrap()
                .0
                .iter()
                .enumerate()
                .map(|(i, &x)| (memos[&curr_bag_id].primary_substations[i], x));
            taps_positions.extend(curr_entry);
            bag_queue.extend(self.td.iter_adjacent_out(&curr_bag_id).unwrap());
        }
        solution_graph_setup(&mut answer, &self.dg_kernel, &taps_positions);
        Some((answer, memos[&self.td.root_id].table.iter().sorted_by_key(|&(_, &x)| x).next().unwrap().1.clone()))
    }

    fn solve(&mut self) -> Result<(), SolverError> {
        // Memos
        let memos: Arc<Mutex<HashMap<usize, TapsMemo>>> = Arc::new(Mutex::new(HashMap::new()));
        // Find out the depth-first search postordering for the bags of self.td
        let mut thread_data = self.td.dfs_postordering().into_iter().map(|id: usize| ThreadMetadata { bag_id: id, join_handle: None, tx: None }).collect_vec();
        // Launch threads with the sliding window
        let mut left_bound: usize = 0;
        let mut right_bound = self.thread_count.min(thread_data.len()) - 1;
        for bag_i in left_bound..=right_bound {
            let input_clone: Arc<SwitchSelectionInstance> = self.input.clone();
            let memos_clone: Arc<Mutex<HashMap<usize, TapsMemo>>> = memos.clone();
            let dtd_clone: Arc<TreeDecomposition> = self.td.clone();
            let bag_id_clone: usize = thread_data[bag_i].bag_id;
            let (tx, rx) = mpsc::channel();
            thread_data[bag_i].join_handle = Some(thread::spawn(move || thread_workload(input_clone, memos_clone, dtd_clone, bag_id_clone, rx)));
            thread_data[bag_i].tx = Some(tx);
        }
        while left_bound <= right_bound {
            thread_data[left_bound].join_handle.take().unwrap().join().unwrap()?;
            left_bound += 1;
            for bag_i in left_bound..=right_bound {
                thread_data[bag_i].tx.as_ref().unwrap().send(thread_data[left_bound - 1].bag_id).unwrap_or(());
            }
            if right_bound + 1 < thread_data.len() {
                right_bound += 1;
                let input_clone: Arc<SwitchSelectionInstance> = self.input.clone();
                let memos_clone: Arc<Mutex<HashMap<usize, TapsMemo>>> = memos.clone();
                let dtd_clone: Arc<TreeDecomposition> = self.td.clone();
                let bag_id_clone: usize = thread_data[right_bound].bag_id;
                let (tx, rx) = mpsc::channel();
                thread_data[right_bound].join_handle = Some(thread::spawn(move || thread_workload(input_clone, memos_clone, dtd_clone, bag_id_clone, rx)));
                thread_data[right_bound].tx = Some(tx);
                for bag_i in 0..left_bound {
                    thread_data[right_bound].tx.as_ref().unwrap().send(thread_data[bag_i].bag_id).unwrap();
                }
            }
        }
        // Save the memos
        self.memos = Some(Arc::into_inner(memos).unwrap().into_inner().unwrap());
        Ok(())
    }
}
