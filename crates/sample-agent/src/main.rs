//! A sample AI agent, fully instrumented with AgentTrace.
//!
//! It simulates realistic agent behavior (no real API keys needed) so the whole
//! AgentTrace pipeline can be demonstrated end to end. It exercises every span
//! type and the "killer demo": a run that calls a model, invokes tools, hits a
//! failing tool, retries, and ultimately fails — with the root cause visible in
//! the viewer.
//!
//! Spans are exported both to a local NDJSON file and (unless `--no-http`) to the
//! collector over HTTP.
//!
//! Usage:
//!   sample-agent [--endpoint URL] [--ndjson PATH] [--count N] [--no-http]

use std::thread::sleep;
use std::time::Duration;

// The sample agent always enables `agenttrace/http`, so the HTTP exporter is
// unconditionally available.
use agenttrace::export::{Exporter, HttpExporter, MultiExporter, NdjsonExporter};
use agenttrace::{SpanType, Tracer};

struct Args {
    endpoint: String,
    ndjson: String,
    count: u32,
    no_http: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        endpoint: std::env::var("AGENTTRACE_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:4357".to_string()),
        ndjson: "traces/sample.ndjson".to_string(),
        count: 2,
        no_http: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--endpoint" => {
                if let Some(v) = it.next() {
                    args.endpoint = v;
                }
            }
            "--ndjson" => {
                if let Some(v) = it.next() {
                    args.ndjson = v;
                }
            }
            "--count" => {
                if let Some(v) = it.next() {
                    args.count = v.parse().unwrap_or(2);
                }
            }
            "--no-http" => args.no_http = true,
            "-h" | "--help" => {
                println!("sample-agent [--endpoint URL] [--ndjson PATH] [--count N] [--no-http]");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    args
}

/// Rough cost estimate using per-million-token prices.
fn estimate_cost(model: &str, input: u64, output: u64) -> f64 {
    let (in_price, out_price) = match model {
        m if m.contains("opus") => (15.0, 75.0),
        m if m.contains("sonnet") => (3.0, 15.0),
        m if m.contains("haiku") => (0.8, 4.0),
        _ => (1.0, 3.0),
    };
    (input as f64 / 1_000_000.0) * in_price + (output as f64 / 1_000_000.0) * out_price
}

fn think(ms: u64) {
    sleep(Duration::from_millis(ms));
}

/// The signature failure demo: model call, two tools, a failing tool, a retry,
/// and a failed run with the root cause attached.
fn failing_research_trace(tracer: &Tracer) -> String {
    let mut run = tracer.start_run("research.agent");
    let trace_id = run.trace_id().to_hex();
    run.set_attr("task", "Summarize today's top AI papers");

    // 1. Model call: plan the task.
    let mut plan = run.child(SpanType::ModelCall, "plan");
    plan.set_model("anthropic", "claude-opus-4");
    think(40);
    let (pin, pout) = (640, 180);
    plan.set_tokens(pin, pout)
        .set_cost(estimate_cost("claude-opus-4", pin, pout));
    plan.end();

    // 2. Tool call: web search (succeeds), with a nested model call to rank.
    let mut search = run.child(SpanType::ToolCall, "web_search");
    search
        .set_tool_name("web_search")
        .set_attr("query", "top AI papers 2026");
    think(30);
    let mut rank = search.child(SpanType::ModelCall, "rank_results");
    rank.set_model("anthropic", "claude-haiku-4");
    think(15);
    let (rin, rout) = (300, 60);
    rank.set_tokens(rin, rout)
        .set_cost(estimate_cost("claude-haiku-4", rin, rout));
    rank.end();
    search.set_attr("result_count", 8);
    search.end();

    // 3. Approval gate before fetching an external URL.
    let mut approval = run.child(SpanType::Approval, "approve_fetch");
    approval
        .set_attr("resource", "https://arxiv.org/…")
        .set_attr("decision", "approved")
        .set_attr("approver", "policy:auto");
    think(5);
    approval.end();

    // 4. Tool call: fetch a page — FAILS with a timeout.
    let mut fetch = run.child(SpanType::ToolCall, "fetch_page");
    fetch
        .set_tool_name("fetch_page")
        .set_attr("url", "https://arxiv.org/abs/2601.01234");
    think(55);
    fetch.set_error("TimeoutError", "GET timed out after 5000ms");
    fetch.end();

    // 5. Retry wrapping a second attempt — also fails.
    let mut retry = run.child(SpanType::Retry, "fetch_page.retry");
    retry.set_retry_count(1).set_attr("backoff_ms", 500);
    let mut fetch2 = retry.child(SpanType::ToolCall, "fetch_page");
    fetch2.set_tool_name("fetch_page");
    think(60);
    fetch2.set_error("TimeoutError", "GET timed out after 5000ms (attempt 2)");
    fetch2.end();
    retry.set_error("TimeoutError", "retry exhausted");
    retry.end();

    // 6. Run fails: error propagates to the root with the root cause.
    run.set_error(
        "ToolFailure",
        "fetch_page failed after 1 retry (TimeoutError)",
    );
    run.end();

    trace_id
}

/// A clean, successful run for contrast.
fn successful_support_trace(tracer: &Tracer, n: u32) -> String {
    let mut run = tracer.start_run("support.agent");
    let trace_id = run.trace_id().to_hex();
    run.set_attr("ticket", format!("SUP-{:04}", 1000 + n));

    let mut model = run.child(SpanType::ModelCall, "classify_intent");
    model.set_model("anthropic", "claude-sonnet-4");
    think(25);
    let (i, o) = (520, 90);
    model
        .set_tokens(i, o)
        .set_cost(estimate_cost("claude-sonnet-4", i, o));
    model.end();

    let mut kb = run.child(SpanType::ToolCall, "search_knowledge_base");
    kb.set_tool_name("search_knowledge_base")
        .set_attr("hits", 3);
    think(20);
    kb.end();

    let mut draft = run.child(SpanType::ModelCall, "draft_reply");
    draft.set_model("anthropic", "claude-sonnet-4");
    think(35);
    let (i, o) = (900, 260);
    draft
        .set_tokens(i, o)
        .set_cost(estimate_cost("claude-sonnet-4", i, o));
    draft.end();

    run.set_attr("resolved", true);
    run.end();
    trace_id
}

fn build_tracer(args: &Args) -> Tracer {
    // Always write NDJSON locally.
    if let Some(parent) = std::path::Path::new(&args.ndjson).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut exporters: Vec<Box<dyn Exporter>> = Vec::new();
    match NdjsonExporter::file(&args.ndjson) {
        Ok(e) => exporters.push(Box::new(e)),
        Err(e) => eprintln!("warning: could not open {}: {e}", args.ndjson),
    }

    if !args.no_http {
        exporters.push(Box::new(HttpExporter::new(args.endpoint.clone())));
        println!("→ exporting spans to collector at {}", args.endpoint);
    }

    Tracer::new(MultiExporter::new(exporters))
}

fn main() {
    let args = parse_args();
    let tracer = build_tracer(&args);

    println!("Running sample agent scenarios…\n");

    let mut ids = Vec::new();
    for n in 0..args.count {
        ids.push(("success", successful_support_trace(&tracer, n)));
    }
    ids.push(("FAILURE", failing_research_trace(&tracer)));

    // Flush exporters (HTTP in particular) before exit.
    tracer.shutdown();

    println!("\nProduced {} traces:", ids.len());
    for (kind, id) in &ids {
        println!("  [{kind:>7}] {id}");
    }
    println!("\nNDJSON written to {}", args.ndjson);
    if !args.no_http {
        println!("Open the viewer at {} to inspect them.", args.endpoint);
    }
}
