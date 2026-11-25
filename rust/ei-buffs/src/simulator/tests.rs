use ei_data::buffs::StackType;

use super::*;

fn apply(time: i64, duration: i64, src: Src) -> SimEvent {
    SimEvent::Apply {
        time,
        duration,
        src,
        src_healing: 0,
        stack_id: 0,
        added_active: false,
        overridden_duration: 0,
        overridden_stack_id: 0,
    }
}

fn run(stack_type: StackType, capacity: usize, events: &[SimEvent], end: i64) -> Simulator {
    let mut sim = Simulator::new(stack_type, capacity);
    sim.simulate(events, 0, end).unwrap();
    sim
}

fn segments(sim: &Simulator) -> Vec<(i64, i64, u32)> {
    sim.generation
        .iter()
        .map(|i| (i.start, i.end, i.active_stacks))
        .collect()
}

#[test]
fn a_duration_stack_runs_out() {
    let sim = run(StackType::Queue, 5, &[apply(100, 1000, 1)], 5000);
    assert_eq!(segments(&sim), vec![(100, 1100, 1)]);
    assert!((uptime(&sim.generation, 0, 5000, false) - 0.2).abs() < 1e-12);
}

#[test]
fn queued_stacks_run_one_after_another() {
    let events = [apply(0, 1000, 1), apply(500, 1000, 2)];
    let sim = run(StackType::Queue, 5, &events, 5000);
    // 0..1000 runs the first stack, the second one waits and runs 1000..2000
    let covered: i64 = sim.generation.iter().map(SimItem::duration).sum();
    assert_eq!(covered, 2000);
    assert_eq!(sim.generation.first().unwrap().sources, vec![1]);
    assert_eq!(sim.generation.last().unwrap().sources, vec![2]);
}

#[test]
fn a_full_queue_drops_the_shortest_waiting_stack() {
    // capacity 2: the third stack replaces the shortest of the waiting ones
    let events = [apply(0, 1000, 1), apply(0, 300, 2), apply(0, 700, 3)];
    let sim = run(StackType::Queue, 2, &events, 5000);
    assert_eq!(
        sim.wastes,
        vec![Waste {
            src: 2,
            value: 300,
            time: 0
        }]
    );
    let covered: i64 = sim.generation.iter().map(SimItem::duration).sum();
    assert_eq!(covered, 1700);
}

#[test]
fn intensity_counts_stacks_and_ends_with_the_shortest() {
    let events = [apply(0, 1000, 1), apply(0, 400, 2)];
    let sim = run(StackType::Stacking, 25, &events, 5000);
    assert_eq!(segments(&sim), vec![(0, 400, 2), (400, 1000, 1)]);
    // average stacks over the first 1000 ms: (2 * 400 + 1 * 600) / 1000
    assert!((uptime(&sim.generation, 0, 1000, true) - 1.4).abs() < 1e-12);
}

#[test]
fn a_full_intensity_buff_replaces_the_shortest_stack() {
    let events = [apply(0, 1000, 1), apply(0, 400, 2), apply(0, 800, 3)];
    let sim = run(StackType::Stacking, 2, &events, 5000);
    assert_eq!(
        sim.wastes,
        vec![Waste {
            src: 2,
            value: 400,
            time: 0
        }]
    );
    assert_eq!(segments(&sim), vec![(0, 800, 2), (800, 1000, 1)]);
}

#[test]
fn force_buffs_are_replaced_by_the_new_stack() {
    let events = [apply(0, 1000, 1), apply(200, 1000, 2)];
    let sim = run(StackType::Force, 1, &events, 5000);
    let covered: i64 = sim.generation.iter().map(SimItem::duration).sum();
    assert_eq!(covered, 1200);
    assert_eq!(sim.wastes.len(), 1);
}

#[test]
fn remove_all_wastes_the_rest_and_ends_the_buff() {
    let events = [
        apply(0, 1000, 1),
        SimEvent::RemoveAll {
            time: 300,
            removed_stacks: 1,
        },
    ];
    let sim = run(StackType::Queue, 5, &events, 5000);
    assert_eq!(segments(&sim), vec![(0, 300, 1)]);
    assert_eq!(
        sim.wastes,
        vec![Waste {
            src: 1,
            value: 700,
            time: 300
        }]
    );
}

#[test]
fn remove_single_matches_the_stack_by_remaining_duration() {
    let events = [
        apply(0, 1000, 1),
        apply(0, 500, 2),
        SimEvent::RemoveSingle {
            time: 100,
            removed_duration: 408,
        },
    ];
    let sim = run(StackType::Stacking, 25, &events, 5000);
    // the 500 ms stack has 400 ms left at 100 ms, within the 15 ms tolerance of 408
    assert_eq!(sim.wastes.len(), 1);
    assert_eq!(sim.wastes[0].src, 2);
}

#[test]
fn extensions_run_after_the_stack() {
    let events = [
        apply(0, 1000, 1),
        SimEvent::Extend {
            time: 500,
            extension: 400,
            old_value: 500,
            src: 2,
            src_healing: 0,
            stack_id: 0,
        },
    ];
    let sim = run(StackType::Queue, 5, &events, 5000);
    let covered: i64 = sim.generation.iter().map(SimItem::duration).sum();
    assert_eq!(covered, 1400);
}

#[test]
fn segments_are_cut_at_the_end_of_the_log() {
    let sim = run(StackType::Queue, 5, &[apply(100, 10_000, 1)], 1000);
    assert_eq!(segments(&sim), vec![(100, 1000, 1)]);
}

#[test]
fn tiny_queues_are_errors() {
    let mut sim = Simulator::new(StackType::Queue, 1);
    let events = [apply(0, 100, 1), apply(0, 100, 2)];
    assert_eq!(
        sim.simulate(&events, 0, 1000).unwrap_err(),
        Error::CapacityTooSmall
    );
}
