use std::{cmp::Ordering, io::{stdout, Write}, time::Instant};
use crabnets::{topology_tests::TopologyTests, BasicImmutableGraph, BasicMutableGraph};
use itertools::Itertools;
use rand::{Rng, distributions::Uniform, prelude::Distribution, seq::IteratorRandom};
use rand_xoshiro::{Xoroshiro128PlusPlus, rand_core::SeedableRng};
use crate::switch_selection_instance::{SwitchSelectionGraph, SwitchSelectionInstance};
use super::{cplex_solver::CPLEXSolver, base_solver::BaseSolver, errors::SolverError, tree_decomposition_solver::TreeDecompositionSolver};





type PRNG = Xoroshiro128PlusPlus;



fn random_partial_k_tree(treewidth: usize, vertex_count: u8, prng: &mut PRNG) -> SwitchSelectionGraph {
    loop {
        let mut answer = SwitchSelectionGraph::new();
        // Populate <answer> with a (treewidth + 1)-clique
        for vertex_id1 in 0..=treewidth {
            answer.add_v(None);
            for vertex_id2 in 0..vertex_id1 {
                answer.add_e(&vertex_id1, &vertex_id2, false, None).unwrap();
            }
        }
        // Add other vertices one by one
        for new_vertex_id in (treewidth + 1)..(vertex_count as usize) {
            answer.add_v(None);
            // Sample a random clique
            let mut clique_candidate;
            'clique_check:
            loop {
                clique_candidate = (0..new_vertex_id).choose_multiple(prng, treewidth);
                for vertex_i1 in 0..treewidth {
                    for vertex_i2 in 0..vertex_i1 {
                        if answer.contains_e(&clique_candidate[vertex_i1], &clique_candidate[vertex_i2], &0).is_none() {
                            continue 'clique_check;
                        }
                    }
                }
                break;
            }
            // Attach the new vertex to the found clique
            for clique_vertex_id in clique_candidate {
                answer.add_e(&new_vertex_id, &clique_vertex_id, false, None).unwrap();
            }
        }
        // Randomly remove some edges
        let edges_to_remove_count: usize = prng.sample(Uniform::new(treewidth, answer.count_e() / 2));
        for _ in 0..edges_to_remove_count {
            let edge = answer.iter_e().sorted_by(|x, y|
                match x.id1.cmp(&y.id1) {
                    Ordering::Less => Ordering::Less,
                    Ordering::Equal => x.id2.cmp(&y.id2),
                    Ordering::Greater => Ordering::Greater,
                }
            ).choose_stable(prng).unwrap();
            if answer.v_degree(&edge.id1).unwrap() > 1 && answer.v_degree(&edge.id2).unwrap() > 1
            && (edge.id1 > treewidth || edge.id2 > treewidth) {
                answer.remove_e(&edge.id1, &edge.id2, &0).unwrap();
            }
        }
        if answer.is_connected() {
            return answer;
        }
    }
}

pub fn start_benchmark(
    treewidth: usize,
    min_primary_substation_count: u8,
    max_primary_substation_count: u8,
    sample_count: u8,
    sample_repeat: u8,
    sample_ignore: u8
) -> Result<(), SolverError> {
    let mut prng = PRNG::seed_from_u64(13374);
    // Random distributions
    let feeder_count_distribution: Uniform<u8> = Uniform::new(2, 6);
    let substation_count_distribution: Uniform<u8> = Uniform::new(5, 11);
    let rx_distribution: Uniform<f64> = Uniform::new(0.01, 0.05);
    // Benchmarking results
    let mut cplex_max_times: Vec<f64> = Vec::with_capacity((max_primary_substation_count - min_primary_substation_count + 1) as usize);
    let mut cplex_avg_times: Vec<f64> = Vec::with_capacity((max_primary_substation_count - min_primary_substation_count + 1) as usize);
    let mut cplex_min_times: Vec<f64> = Vec::with_capacity((max_primary_substation_count - min_primary_substation_count + 1) as usize);
    let mut td_max_times: Vec<f64> = Vec::with_capacity((max_primary_substation_count - min_primary_substation_count + 1) as usize);
    let mut td_avg_times: Vec<f64> = Vec::with_capacity((max_primary_substation_count - min_primary_substation_count + 1) as usize);
    let mut td_min_times: Vec<f64> = Vec::with_capacity((max_primary_substation_count - min_primary_substation_count + 1) as usize);
    // Test random distribution grids
    for primary_substation_count in min_primary_substation_count..=max_primary_substation_count {
        println!("Treewidth = {}, # primary substations = {}", treewidth, primary_substation_count);
        let mut cplex_times: Vec<f64> = Vec::with_capacity(sample_count as usize);
        let mut td_times: Vec<f64> = Vec::with_capacity(sample_count as usize);
        let mut successful_samples: u8 = 0;
        let mut restart = false;
        loop {
            if restart {
                print!("X");
                stdout().flush().unwrap();
            } else {
                print!("\tSample {}\n\t\t", successful_samples + 1);
            }
            // Generate a random partial k-tree
            let mut graph: SwitchSelectionGraph = random_partial_k_tree(treewidth, primary_substation_count, &mut prng);
            print!("P");
            stdout().flush().unwrap();
            // Replace each edge of the graph with a bunch of feeders
            let old_edges = graph.iter_e().sorted_by(|x, y|
                match x.id1.cmp(&y.id1) {
                    Ordering::Less => Ordering::Less,
                    Ordering::Equal => x.id2.cmp(&y.id2),
                    Ordering::Greater => Ordering::Greater,
                }
            );
            for old_edge in old_edges {
                let feeder_count: u8 = feeder_count_distribution.sample(&mut prng);
                graph.v_attrs_mut(&old_edge.id1).unwrap().tap_position = Some(0);
                graph.v_attrs_mut(&old_edge.id2).unwrap().tap_position = Some(0);
                graph.remove_e(&old_edge.id1, &old_edge.id2, &0).unwrap();
                for _ in 0..feeder_count {
                    let substation_count: u8 = substation_count_distribution.sample(&mut prng);
                    let mut last_substation_id = old_edge.id1;
                    let pq_distribution: Uniform<f64> = Uniform::new(-0.5, 0.5);
                    for _ in 0..substation_count {
                        let new_substation_id: usize = graph.add_v(None);
                        {
                            let attributes = graph.v_attrs_mut(&new_substation_id).unwrap();
                            attributes.p = pq_distribution.sample(&mut prng);
                            attributes.q = pq_distribution.sample(&mut prng);
                        }
                        graph.add_e(&last_substation_id, &new_substation_id, false, None).unwrap();
                        {
                            let attributes = graph.e_attrs_mut(&last_substation_id, &new_substation_id, &0).unwrap();
                            attributes.r = rx_distribution.sample(&mut prng);
                            attributes.x = rx_distribution.sample(&mut prng);
                        }
                        last_substation_id = new_substation_id;
                    }
                    graph.add_e(&last_substation_id, &old_edge.id2, false, None).unwrap();
                    {
                        let attributes = graph.e_attrs_mut(&last_substation_id, &old_edge.id2, &0).unwrap();
                        attributes.r = rx_distribution.sample(&mut prng);
                        attributes.x = rx_distribution.sample(&mut prng);
                    }
                }
            }
            print!("D");
            stdout().flush().unwrap();
            // Create a problem instance instance out of graph
            let instance = SwitchSelectionInstance::new(graph).unwrap();
            // Time the TreeDecompositionSolver
            let mut solver: TreeDecompositionSolver = TreeDecompositionSolver::with_input(instance.clone())?;
            match timeit(&mut solver, sample_repeat, sample_ignore) {
                Ok(value) => td_times.push(value),
                Err(_) => {
                    restart = true;
                    continue;
                },
            }
            println!("\n\t\tTreeDecompositionSolver finished ({} s). Optimal value = {}.", td_times.last().unwrap(), solver.get_solution().unwrap().1);
            // Time the CPLEXSolver
            let mut solver: CPLEXSolver = CPLEXSolver::with_input(instance.clone())?;
            match timeit(&mut solver, sample_repeat, sample_ignore) {
                Ok(value) => cplex_times.push(value),
                Err(_) => {
                    restart = true;
                    continue;
                },
            }
            println!("\t\tCPLEXSolver finished ({} s). Optimal value = {}.", cplex_times.last().unwrap(), solver.get_solution().unwrap().1);
            successful_samples += 1;
            restart = false;
            if successful_samples == sample_count {
                break;
            }
        }
        // Print the results for this number of primary substations
        println!("\tResults for this # primary substations");
        println!("\tTreeDecompositionSolver: {:?}", td_times);
        println!("\tCPLEXSolver: {:?}", cplex_times);
        // Remember the results for this number of primary substations
        cplex_max_times.push(cplex_times.iter().map(|x: &f64| *x).reduce(f64::max).unwrap());
        cplex_avg_times.push(cplex_times.iter().sum::<f64>() / sample_count as f64);
        cplex_min_times.push(cplex_times.iter().map(|x: &f64| *x).reduce(f64::min).unwrap());
        td_max_times.push(td_times.iter().map(|x: &f64| *x).reduce(f64::max).unwrap());
        td_avg_times.push(td_times.iter().sum::<f64>() / sample_count as f64);
        td_min_times.push(td_times.iter().map(|x: &f64| *x).reduce(f64::min).unwrap());
    }
    println!("Results");
    println!("TreeDecompositionSolver max: {:?}", td_max_times);
    println!("TreeDecompositionSolver avg: {:?}", td_avg_times);
    println!("TreeDecompositionSolver min: {:?}", td_min_times);
    println!("CPLEXSolver max: {:?}", cplex_max_times);
    println!("CPLEXSolver avg: {:?}", cplex_avg_times);
    println!("CPLEXSolver min: {:?}", cplex_min_times);
    Ok(())
}

pub fn timeit<S: BaseSolver>(solver: &mut S, repeat: u8, ignore: u8) -> Result<f64, SolverError> {
    for _ in 0..ignore {
        solver.solve()?;
    }
    let mut cumulative_time: f64 = 0.0;
    for _ in 0..(repeat - ignore) {
        let solver_begin_time: Instant = Instant::now();
        solver.solve()?;
        cumulative_time += solver_begin_time.elapsed().as_secs_f64();
    }
    Ok(cumulative_time / repeat as f64)
}
