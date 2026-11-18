fn main() {
    if std::env::var_os("CARGO_FEATURE_PLUGIN").is_none() {
        return;
    }

    // Must match FLECS_DEFINES in the plugin Makefile, or struct layouts diverge from flecs_ecs's.
    let defines = [
        "FLECS_CUSTOM_BUILD", "FLECS_CPP", "FLECS_MODULE", "FLECS_SYSTEM", "FLECS_PIPELINE",
        "FLECS_TIMER", "FLECS_OS_API_IMPL", "FLECS_MUT_ALIAS_LOCKS", "FLECS_SCRIPT", "FLECS_STATS",
        "FLECS_METRICS", "FLECS_ALERTS", "FLECS_META", "FLECS_META_C", "FLECS_UNITS", "FLECS_JSON",
        "FLECS_DOC", "FLECS_LOG", "FLECS_APP", "FLECS_HTTP", "FLECS_REST",
    ];

    let mut build = cc::Build::new();
    build
        .file("c/xast_sched.c")
        .include("c")
        .define("FLECS_TERM_COUNT_MAX", "32")
        .flag("-pthread");

    for define in defines {
        build.define(define, None);
    }

    build.compile("xast_sched");

    println!("cargo:rerun-if-changed=c/xast_sched.c");
    println!("cargo:rerun-if-changed=c/xast_sched.h");
}
