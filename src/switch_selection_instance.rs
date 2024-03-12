use std::{cmp::Ordering, collections::VecDeque, iter::once};
use crabnets::{attributes::*, io::{AttributeCollectionIO, AttributeToken}, locales::*, topology_tests::TopologyTests, *};
use itertools::Itertools;
use crate::solver::errors::GraphError;






#[derive(Clone, Default)]
pub struct DGVertexAttributes {
    pub line_endpoints: Option<(usize, usize)>,
    pub p: f64,
    pub q: f64,
    pub tap_position: Option<i8>,
}

// DGVertexAttributes::AttributeCollection
impl AttributeCollection for DGVertexAttributes {
    fn new() -> Self {
        DGVertexAttributes { line_endpoints: None, p: 0.0, q: 0.0, tap_position: None }
    }
}

// DGVertexAttributes::AttributeCollectionIO
impl AttributeCollectionIO for DGVertexAttributes {
    fn io_iter_contents<'a>(&'a self) -> Box<dyn Iterator<Item = AttributeToken<'a>> + 'a> {
        let tap_position_data = self.tap_position.is_some().then(
            || once(AttributeToken { name: "tap position", value: StaticDispatchAttributeValue::Int8(self.tap_position.unwrap()) })
        ).into_iter().flatten();
        Box::new(
            once(AttributeToken { name: "p", value: StaticDispatchAttributeValue::Float64(self.p) })
            .chain(once(AttributeToken { name: "q", value: StaticDispatchAttributeValue::Float64(self.q) }))
            .chain(tap_position_data)
        )
    }

    fn io_query_contents(&self, attribute_name: &str) -> Option<StaticDispatchAttributeValue> {
        match attribute_name {
            "p" => Some(StaticDispatchAttributeValue::Float64(self.p)),
            "q" => Some(StaticDispatchAttributeValue::Float64(self.q)),
            "tap position" => match self.tap_position {
                Some(value) => Some(StaticDispatchAttributeValue::Int8(value)),
                None => None,
            },
            _ => None,
        }
    }

    fn io_reader_callback<'a, EdgeIdType, VertexIdType>(&mut self, token: AttributeToken<'a>)
        where
            EdgeIdType: Id,
            VertexIdType: Id
    {
        match token.name {
            "p" => if let StaticDispatchAttributeValue::Float64(value) = token.value {
                self.p = value;
            },
            "q" => if let StaticDispatchAttributeValue::Float64(value) = token.value {
                self.q = value;
            },
            "is primary substation" => if let StaticDispatchAttributeValue::Bool(value) = token.value {
                self.tap_position = if value {
                    Some(0)
                } else {
                    None
                }
            },
            _ => (),
        }
    }
}



#[derive(Clone, Default)]
pub struct DGEdgeAttributes {
    pub line_endpoints: Option<(usize, usize)>,
    pub r: f64,
    pub switch: bool,
    pub x: f64,
}

// DGEdgeAttributes::AttributeCollection
impl AttributeCollection for DGEdgeAttributes {
    fn new() -> Self {
        DGEdgeAttributes { line_endpoints: None, r: 0.0, switch: false, x: 0.0 }
    }
}

// DGEdgeAttributes::AttributeCollectionIO
impl AttributeCollectionIO for DGEdgeAttributes {
    fn io_iter_contents<'a>(&'a self) -> Box<dyn Iterator<Item = AttributeToken<'a>> + 'a> {
        Box::new(
            once(AttributeToken { name: "r", value: StaticDispatchAttributeValue::Float64(self.r) })
            .chain(once(AttributeToken { name: "opened switch", value: StaticDispatchAttributeValue::Bool(self.switch) }))
            .chain(once(AttributeToken { name: "x", value: StaticDispatchAttributeValue::Float64(self.x) }))
        )
    }

    fn io_query_contents(&self, attribute_name: &str) -> Option<StaticDispatchAttributeValue> {
        match attribute_name {
            "r" => Some(StaticDispatchAttributeValue::Float64(self.r)),
            "opened switch" => Some(StaticDispatchAttributeValue::Bool(self.switch)),
            "x" => Some(StaticDispatchAttributeValue::Float64(self.x)),
            _ => None,
        }
    }

    fn io_reader_callback<'a, EdgeIdType, VertexIdType>(&mut self, token: AttributeToken<'a>)
        where
            EdgeIdType: Id,
            VertexIdType: Id,
    {
        match token.name {
            "r" => if let StaticDispatchAttributeValue::Float64(value) = token.value {
                self.r = value;
            },
            "x" => if let StaticDispatchAttributeValue::Float64(value) = token.value {
                self.x = value;
            },
            _ => (),
        }
    }
}



pub type SwitchSelectionGraph = graph!(A ---A--- A with VertexAttributeCollectionType = DGVertexAttributes, EdgeAttributeCollectionType = DGEdgeAttributes);



#[derive(Clone, Default)]
pub struct SwitchSelectionInstance {
    graph: SwitchSelectionGraph,
}

// SwitchSelectionInstance::SwitchSelectionInstance
impl SwitchSelectionInstance {
    pub fn new(mut graph: SwitchSelectionGraph) -> Result<Self, GraphError> {
        if !graph.is_connected() {
            return Err(GraphError::from_str("The given ditribution grid is not connected."));
        }
        let mut unvisited_primary_substations: VecDeque<usize> = VecDeque::from_iter(graph.iter_v().filter(|x| graph.v_attrs(x).unwrap().tap_position.is_some()));
        if unvisited_primary_substations.is_empty() {
            return Err(GraphError::from_str("The given distribution grid doesn't contain any primary substations."));
        }
        // Launch depth-first search from each primary substation to  determine
        // which secondary substation belongs to a line between  which  primary
        // substations.
        while !unvisited_primary_substations.is_empty() {
            let primary_substation_id = unvisited_primary_substations.pop_front().unwrap();
            let mut unvisited_vertices_stack = VecDeque::from_iter(
                graph.iter_adjacent(&primary_substation_id).unwrap().filter(|x|
                    graph.v_attrs(x).unwrap().line_endpoints.is_none()
                )
            );
            let mut curr_line = Vec::from([primary_substation_id]);
            while !unvisited_vertices_stack.is_empty() {
                let curr_substation_id = unvisited_vertices_stack.pop_front().unwrap();
                curr_line.push(curr_substation_id);
                // If the current substation is a primary substation, we've reached the end of  the
                // line.  Backtrack   and   set   line_endpoints   to   primary_substation_id   and
                // curr_substation_id.
                if graph.v_attrs(&curr_substation_id).unwrap().tap_position.is_some() {
                    let line_endpoints = match primary_substation_id.cmp(&curr_substation_id) {
                        Ordering::Less => (primary_substation_id, curr_substation_id),
                        Ordering::Equal => return Err(GraphError::from_string(format!("Primary substation {} has a feeder that begins and ends in it.", curr_substation_id))),
                        Ordering::Greater => (curr_substation_id, primary_substation_id),
                    };
                    graph.e_attrs_mut(curr_line.last().unwrap(), &curr_line[curr_line.len() - 2], &0).unwrap().line_endpoints = Some(line_endpoints.clone());
                    for substation_i in (1..(curr_line.len() - 1)).rev() {
                        graph.v_attrs_mut(&curr_line[substation_i]).unwrap().line_endpoints = Some(line_endpoints.clone());
                        graph.e_attrs_mut(&curr_line[substation_i], &curr_line[substation_i - 1], &0).unwrap().line_endpoints = Some(line_endpoints.clone());
                    }
                    curr_line.resize(1, 0);
                    continue;
                }
                // If the current substation is a secondary substation, check that it has exactly 2
                // neighbours and add the unvisited one on top of the stack.
                let adjacent_substations = graph.iter_adjacent(&curr_substation_id).unwrap().collect_vec();
                if adjacent_substations.len() != 2 {
                    return Err(GraphError::from_string(format!("Secondary substation {} must have exactly 2 adjacent substations.", curr_substation_id)));
                }
                unvisited_vertices_stack.push_front(if adjacent_substations[0] != curr_line[curr_line.len() - 2] { adjacent_substations[0] } else { adjacent_substations[1] });
            }
        }
        Ok(SwitchSelectionInstance { graph })
    }

    pub fn dg_kernel_for_switch_selection(&self) -> SwitchSelectionGraph {
        let mut answer = SwitchSelectionGraph::new();
        let lines = self.graph.iter_e().map(|x| self.graph.e_attrs(&x.id1, &x.id2, &x.edge_id).unwrap().line_endpoints.unwrap()).unique();
        for line in lines {
            if !answer.contains_v(&line.0) {
                answer.add_v(Some(line.0));
            }
            if !answer.contains_v(&line.1) {
                answer.add_v(Some(line.1));
            }
            answer.add_e(&line.0, &line.1, false, None).unwrap();
        }
        answer
    }
}

// SwitchSelectionInstance::ImmutableGraphContainer
impl ImmutableGraphContainer for SwitchSelectionInstance {
    type EdgeAttributeCollectionType = DGEdgeAttributes;
    type EdgeIdType = u8;
    type LocaleType = SimpleUndirectedLocale<DGEdgeAttributes, DGVertexAttributes, usize>;
    type VertexAttributeCollectionType = DGVertexAttributes;
    type VertexIdType = usize;

    fn unwrap(&self) -> &Graph<Self::EdgeAttributeCollectionType, Self::EdgeIdType, Self::LocaleType, Self::VertexAttributeCollectionType, Self::VertexIdType> {
        &self.graph
    }
}

// SwitchSelectionInstance::MutableGraphContainer
impl MutableGraphContainer for SwitchSelectionInstance {
    fn unwrap(&mut self) -> &mut Graph<Self::EdgeAttributeCollectionType, Self::EdgeIdType, Self::LocaleType, Self::VertexAttributeCollectionType, Self::VertexIdType> {
        &mut self.graph
    }
}
