use itertools::Itertools;

use crate::{
    compiler::hir::Id,
    module::Module,
    vm::{ChannelId, FiberId, Heap, Pointer},
};
use std::{collections::HashMap, fmt, time::Instant};

use super::{FiberEvent, Tracer, VmEvent};

/// A full tracer that saves all events that occur with timestamps.
#[derive(Clone, Default)]
pub struct FullTracer {
    pub events: Vec<TimedEvent>,
    pub heap: Heap,
    transferred_objects: HashMap<FiberId, HashMap<Pointer, Pointer>>,
}
#[derive(Clone)]
pub struct TimedEvent {
    pub when: Instant,
    pub event: StoredVmEvent,
}

#[derive(Clone)]
pub enum StoredVmEvent {
    FiberCreated {
        fiber: FiberId,
    },
    FiberDone {
        fiber: FiberId,
    },
    FiberPanicked {
        fiber: FiberId,
        panicked_child: Option<FiberId>,
    },
    FiberCanceled {
        fiber: FiberId,
    },
    FiberExecutionStarted {
        fiber: FiberId,
    },
    FiberExecutionEnded {
        fiber: FiberId,
    },
    ChannelCreated {
        channel: ChannelId,
    },
    InFiber {
        fiber: FiberId,
        event: StoredFiberEvent,
    },
}
#[derive(Clone)]
pub enum StoredFiberEvent {
    ModuleStarted {
        module: Module,
    },
    ModuleEnded {
        export_map: Pointer,
    },
    ValueEvaluated {
        id: Id,
        value: Pointer,
    },
    FoundFuzzableClosure {
        id: Id,
        closure: Pointer,
    },
    CallStarted {
        id: Id,
        closure: Pointer,
        args: Vec<Pointer>,
    },
    CallEnded {
        return_value: Pointer,
    },
    NeedsStarted {
        id: Id,
        condition: Pointer,
        reason: Pointer,
    },
    NeedsEnded,
}

impl Tracer for FullTracer {
    fn add(&mut self, event: VmEvent) {
        let event = TimedEvent {
            when: Instant::now(),
            event: self.map_vm_event(event),
        };
        self.events.push(event);
    }
}
impl FullTracer {
    fn import_from_heap(
        &mut self,
        address: Pointer,
        heap: &Heap,
        fiber: Option<FiberId>,
    ) -> Pointer {
        if let Some(fiber) = fiber {
            let map = self
                .transferred_objects
                .entry(fiber)
                .or_insert_with(HashMap::new);
            heap.clone_single_to_other_heap_with_existing_mapping(&mut self.heap, address, map)
        } else {
            heap.clone_single_to_other_heap(&mut self.heap, address)
        }
    }

    fn map_vm_event(&mut self, event: VmEvent) -> StoredVmEvent {
        match event {
            VmEvent::FiberCreated { fiber } => StoredVmEvent::FiberCreated { fiber },
            VmEvent::FiberDone { fiber } => StoredVmEvent::FiberDone { fiber },
            VmEvent::FiberPanicked {
                fiber,
                panicked_child,
            } => StoredVmEvent::FiberPanicked {
                fiber,
                panicked_child,
            },
            VmEvent::FiberCanceled { fiber } => StoredVmEvent::FiberCanceled { fiber },
            VmEvent::FiberExecutionStarted { fiber } => {
                StoredVmEvent::FiberExecutionStarted { fiber }
            }
            VmEvent::FiberExecutionEnded { fiber } => StoredVmEvent::FiberExecutionEnded { fiber },
            VmEvent::ChannelCreated { channel } => StoredVmEvent::ChannelCreated { channel },
            VmEvent::InFiber { fiber, event } => StoredVmEvent::InFiber {
                fiber,
                event: self.map_fiber_event(event, fiber),
            },
        }
    }
    fn map_fiber_event(&mut self, event: FiberEvent, fiber: FiberId) -> StoredFiberEvent {
        match event {
            FiberEvent::ModuleStarted { module } => StoredFiberEvent::ModuleStarted { module },
            FiberEvent::ModuleEnded { export_map, heap } => {
                let export_map = self.import_from_heap(export_map, heap, Some(fiber));
                StoredFiberEvent::ModuleEnded { export_map }
            }
            FiberEvent::ValueEvaluated { id, value, heap } => {
                let value = self.import_from_heap(value, heap, Some(fiber));
                StoredFiberEvent::ValueEvaluated { id, value }
            }
            FiberEvent::FoundFuzzableClosure { id, closure, heap } => {
                let closure = self.import_from_heap(closure, heap, Some(fiber));
                StoredFiberEvent::FoundFuzzableClosure { id, closure }
            }
            FiberEvent::CallStarted {
                id,
                closure,
                args,
                heap,
            } => {
                let closure = self.import_from_heap(closure, heap, Some(fiber));
                let args = args
                    .into_iter()
                    .map(|arg| self.import_from_heap(arg, heap, Some(fiber)))
                    .collect();
                StoredFiberEvent::CallStarted { id, closure, args }
            }
            FiberEvent::CallEnded { return_value, heap } => {
                let return_value = self.import_from_heap(return_value, heap, Some(fiber));
                StoredFiberEvent::CallEnded { return_value }
            }
            FiberEvent::NeedsStarted {
                id,
                condition,
                reason,
                heap,
            } => {
                let condition = self.import_from_heap(condition, heap, Some(fiber));
                let reason = self.import_from_heap(reason, heap, Some(fiber));
                StoredFiberEvent::NeedsStarted {
                    id,
                    condition,
                    reason,
                }
            }
            FiberEvent::NeedsEnded => StoredFiberEvent::NeedsEnded,
        }
    }
}

impl fmt::Debug for FullTracer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = self.events.first().map(|event| event.when);
        for event in &self.events {
            writeln!(
                f,
                "{:?} µs: {}",
                event.when.duration_since(start.unwrap()).as_micros(),
                match &event.event {
                    StoredVmEvent::FiberCreated { fiber } => format!("{fiber:?}: created"),
                    StoredVmEvent::FiberDone { fiber } => format!("{fiber:?}: done"),
                    StoredVmEvent::FiberPanicked {
                        fiber,
                        panicked_child,
                    } => format!(
                        "{fiber:?}: panicked{}",
                        if let Some(child) = panicked_child {
                            format!(" because child {child:?} panicked")
                        } else {
                            "".to_string()
                        }
                    ),
                    StoredVmEvent::FiberCanceled { fiber } => format!("{fiber:?}: canceled"),
                    StoredVmEvent::FiberExecutionStarted { fiber } =>
                        format!("{fiber:?}: execution started"),
                    StoredVmEvent::FiberExecutionEnded { fiber } =>
                        format!("{fiber:?}: execution ended"),
                    StoredVmEvent::ChannelCreated { channel } => format!("{channel:?}: created"),
                    StoredVmEvent::InFiber { fiber, event } => format!(
                        "{fiber:?}: {}",
                        match event {
                            StoredFiberEvent::ModuleStarted { module } =>
                                format!("module {module} started"),
                            StoredFiberEvent::ModuleEnded { export_map } => format!(
                                "module ended and exported {}",
                                export_map.format(&self.heap)
                            ),
                            StoredFiberEvent::ValueEvaluated { id, value } =>
                                format!("value {id} is {}", value.format(&self.heap)),
                            StoredFiberEvent::FoundFuzzableClosure { id, .. } =>
                                format!("found fuzzable closure {id}"),
                            StoredFiberEvent::CallStarted { id, closure, args } => format!(
                                "call {id} started: {} {}",
                                closure.format(&self.heap),
                                args.iter().map(|arg| arg.format(&self.heap)).join(" ")
                            ),
                            StoredFiberEvent::CallEnded { return_value } =>
                                format!("call ended: {}", return_value.format(&self.heap)),
                            StoredFiberEvent::NeedsStarted {
                                id,
                                condition,
                                reason,
                            } => format!(
                                "needs {id} started: needs {} {}",
                                condition.format(&self.heap),
                                reason.format(&self.heap)
                            ),
                            StoredFiberEvent::NeedsEnded => "needs ended".to_string(),
                        }
                    ),
                }
            )?;
        }
        Ok(())
    }
}
