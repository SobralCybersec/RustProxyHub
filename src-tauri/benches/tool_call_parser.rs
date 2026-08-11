use std::{hint::black_box, time::Instant};

use tauri_app_lib::proxy_core::StreamingToolParser;

const ITERATIONS: usize = 10_000;
const FIXTURE: [&str; 2] = [
    "before <tool_call name=\"report_smoke_target\"><parameter name=\"provider\">qwen</parameter>",
    "<parameter name=\"model\">qwen3</parameter></tool_call> after",
];

fn main() {
    for _ in 0..1_000 {
        parse_fixture();
    }

    let started = Instant::now();
    let tool_calls: usize = (0..ITERATIONS).map(|_| parse_fixture()).sum();
    let elapsed = started.elapsed();
    let operations_per_second = ITERATIONS as f64 / elapsed.as_secs_f64();

    println!(
        "{{\"benchmark\":\"streaming_tool_call_parser\",\"elapsed_ms\":{:.3},\"iterations\":{ITERATIONS},\"operations_per_second\":{operations_per_second:.2},\"tool_calls\":{tool_calls}}}",
        elapsed.as_secs_f64() * 1_000.0
    );
}

fn parse_fixture() -> usize {
    let mut parser = StreamingToolParser::new();
    let first = parser.feed(black_box(FIXTURE[0]));
    let second = parser.feed(black_box(FIXTURE[1]));
    let flushed = parser.flush();
    black_box(first.tool_calls.len() + second.tool_calls.len() + flushed.tool_calls.len())
}
