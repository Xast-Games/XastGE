#ifndef XAST_SCHED_H
#define XAST_SCHED_H

#include <stdint.h>
#include "flecs.h"

typedef enum XastPhase {
    XAST_PHASE_SETUP = 0,
    XAST_PHASE_UPDATE = 1,
    XAST_PHASE_RENDER = 2,
    XAST_PHASE_COUNT
} XastPhase;

#define XAST_MAX_SYSTEMS_PER_PHASE 8
#define XAST_MAX_UNITS 16

/* Owned by the host (the engine, or a standalone program's main). A unit is one
 * compilation unit of Xast systems: a plugin, or a whole standalone program.
 * Units run one after another in creation order; systems inside a unit may run in
 * parallel, which is exactly the scope the Xast compiler's conflict proof covers. */
typedef struct XastSched XastSched;

XastSched *xast_sched_new(ecs_world_t *world, int workers);
uint32_t xast_sched_new_unit(XastSched *sched);
void xast_sched_register(XastSched *sched, uint32_t unit, XastPhase phase, ecs_entity_t system);
void xast_sched_remove_unit(XastSched *sched, uint32_t unit);
void xast_sched_run_phase(XastSched *sched, XastPhase phase, ecs_ftime_t dt);

#endif
