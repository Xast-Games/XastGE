#include "xast_sched.h"

#include <pthread.h>
#include <stdbool.h>
#include <stdlib.h>

typedef struct XastUnit {
    bool active;
    int32_t count[XAST_PHASE_COUNT];
    ecs_entity_t systems[XAST_PHASE_COUNT][XAST_MAX_SYSTEMS_PER_PHASE];
} XastUnit;

struct XastSched {
    ecs_world_t *world;
    uint32_t unit_count;
    XastUnit units[XAST_MAX_UNITS];
};

XastSched *xast_sched_new(ecs_world_t *world, int workers) {
    (void)workers; /* reserved for the worker pool; for now one thread per system */

    XastSched *sched = calloc(1, sizeof(XastSched));
    ecs_assert(sched != NULL, ECS_OUT_OF_MEMORY, NULL);
    sched->world = world;
    return sched;
}

uint32_t xast_sched_new_unit(XastSched *sched) {
    ecs_assert(sched->unit_count < XAST_MAX_UNITS, ECS_INVALID_PARAMETER, "too many xast units");

    uint32_t unit = sched->unit_count++;
    sched->units[unit].active = true;
    return unit;
}

void xast_sched_register(XastSched *sched, uint32_t unit, XastPhase phase, ecs_entity_t system) {
    ecs_assert(unit < sched->unit_count && sched->units[unit].active, ECS_INVALID_PARAMETER, "unknown xast unit");

    XastUnit *u = &sched->units[unit];
    ecs_assert(u->count[phase] < XAST_MAX_SYSTEMS_PER_PHASE,
        ECS_INVALID_PARAMETER, "too many xast systems registered for one phase");
    u->systems[phase][u->count[phase]++] = system;
}

/* Unit ids stay stable so the run order of the others doesn't change. */
void xast_sched_remove_unit(XastSched *sched, uint32_t unit) {
    ecs_assert(unit < sched->unit_count, ECS_INVALID_PARAMETER, "unknown xast unit");

    XastUnit *u = &sched->units[unit];
    u->active = false;
    for (int32_t p = 0; p < XAST_PHASE_COUNT; p++) {
        u->count[p] = 0;
    }
}

typedef struct XastRunArgs {
    ecs_world_t *stage;
    ecs_entity_t system;
    ecs_ftime_t dt;
} XastRunArgs;

static void *xast_run_thread(void *arg) {
    XastRunArgs *args = (XastRunArgs *)arg;
    ecs_run(args->stage, args->system, args->dt, NULL);
    return NULL;
}

/* Runs one unit's systems of one phase. Needs exclusive access to `world` for its
 * duration, same as ecs_run_pipeline() would. */
static void xast_run_group(ecs_world_t *world, const ecs_entity_t *systems, int32_t count, ecs_ftime_t dt) {
    /* Stage count is global world state, shared with the host's own
     * pipeline execution -- ecs_run_pipeline() decides whether to wait on
     * flecs's own worker-thread pool based on ecs_get_stage_count() > 1,
     * regardless of whether that pool actually exists (it only does if the
     * host called ecs_set_threads()/ecs_set_task_threads()). Leaving the
     * count raised after this call returns makes the *next* native
     * pipeline run try to sync with worker threads that were never
     * created, segfaulting on a null mutex. So: save it, and restore it
     * before returning.
     *
     * Stage count must also be set *before* entering readonly mode --
     * stages created afterwards aren't properly deferred and corrupt the
     * merge in ecs_readonly_end(). */
    int32_t prev_stage_count = ecs_get_stage_count(world);
    ecs_set_stage_count(world, count);
    ecs_readonly_begin(world, true);
    /* Multi-threaded readonly mode requires every access to go through a
     * stage, even when a phase's group is a single system with no thread
     * hand-off -- the bare `world` pointer is not valid to run systems
     * through once multi-threaded readonly is active. */

    if (count == 1) {
        ecs_world_t *stage = ecs_get_stage(world, 0);
        ecs_run(stage, systems[0], dt, NULL);
    } else {
        /* Compile-time analysis already proved these don't conflict --
         * dispatch each on its own thread against its own stage. */
        pthread_t threads[XAST_MAX_SYSTEMS_PER_PHASE];
        XastRunArgs args[XAST_MAX_SYSTEMS_PER_PHASE];

        for (int32_t i = 0; i < count; i++) {
            args[i].stage = ecs_get_stage(world, i);
            args[i].system = systems[i];
            args[i].dt = dt;
            pthread_create(&threads[i], NULL, xast_run_thread, &args[i]);
        }

        for (int32_t i = 0; i < count; i++) {
            pthread_join(threads[i], NULL);
        }
    }

    ecs_readonly_end(world);
    ecs_set_stage_count(world, prev_stage_count);
}

void xast_sched_run_phase(XastSched *sched, XastPhase phase, ecs_ftime_t dt) {
    for (uint32_t u = 0; u < sched->unit_count; u++) {
        const XastUnit *unit = &sched->units[u];
        if (unit->active && unit->count[phase] > 0) {
            xast_run_group(sched->world, unit->systems[phase], unit->count[phase], dt);
        }
    }
}
