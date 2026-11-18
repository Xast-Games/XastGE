use std::{any::TypeId, collections::HashSet};

use flecs_ecs::prelude::*;
use glam::Vec2;

use crate::{
    Update,
    app::{input::Mouse, window::{MouseButton, WindowSize}},
    render::registry::RenderRegistry,
};

use super::{
    UiFonts, UiModule,
    element::Element,
    tree::{Pointer, Resources, UiEvent, UiHandlers, UiTree},
};

pub trait UiApp: Send + Sync + 'static {
    type Msg: Clone + Send + Sync + 'static;
    type Effect: Send + Sync + 'static;

    fn update(&mut self, msg: Self::Msg, ctx: &mut Ctx<Self::Effect>);

    fn view(&self) -> Element<Self::Msg>;

    /// Messages sent to `inbox` are delivered next tick.
    fn perform(effect: Self::Effect, world: &World, inbox: &mut Inbox<Self::Msg>) {
        let _ = (effect, world, inbox);
    }
}

pub struct Ctx<'a, E> {
    effects: &'a mut Vec<E>,
    time: f64,
}

impl<E> Ctx<'_, E> {
    pub fn emit(&mut self, effect: E) {
        self.effects.push(effect);
    }

    pub fn time(&self) -> f64 {
        self.time
    }
}

pub struct Inbox<M> {
    msgs: Vec<M>,
}

impl<M> Inbox<M> {
    pub fn send(&mut self, msg: M) {
        self.msgs.push(msg);
    }
}

#[derive(Component)]
pub struct UiRoot<A: UiApp> {
    app: A,
    inbox: Vec<A::Msg>,
    view_dirty: bool,
    time: f64,
}

impl<A: UiApp> UiRoot<A> {
    pub fn new(app: A) -> Self {
        UiRoot {
            app,
            inbox: Vec::new(),
            view_dirty: true,
            time: 0.0,
        }
    }

    pub fn model(&self) -> &A {
        &self.app
    }

    pub fn send(&mut self, msg: A::Msg) {
        self.inbox.push(msg);
    }
}

// Not a generic Module: flecs-rs doesn't distinguish Module<A> from Module<B>
#[derive(Component, Default)]
pub(crate) struct UiApps {
    registered: HashSet<TypeId>,
}

fn register_app<A: UiApp>(world: &World) {
    world.import::<UiModule>();

    let fresh = world.get::<&mut UiApps>(|apps| apps.registered.insert(TypeId::of::<A>()));
    if !fresh {
        return;
    }

    assert!(
        !world.is_deferred(),
        "UI app `{}` must be registered outside of running systems (e.g. in a Module or before `run`), \
         call `register_ui_app` there before spawning it from a system",
        std::any::type_name::<A>(),
    );

    world.component::<UiRoot<A>>();
    world.component::<UiHandlers<A::Msg>>();

    let roots = world.query::<&UiRoot<A>>().build();

    // Immediate systems still run deferred; the view diff needs its writes visible
    world.system::<()>()
        .kind(Update)
        .immediate(true)
        .run(move |it| {
            let world = it.world();
            let dt = it.delta_time() as f64;

            let deferred = world.is_deferred();
            if deferred {
                world.defer_suspend();
            }

            let mut entities = Vec::new();
            roots.each_entity(|e, _| entities.push(e.id()));

            let viewport = world.get::<&WindowSize>(|s| Vec2::new(s.width(), s.height()));
            let pointer = world.get::<&Mouse>(|m| {
                let pos = m.position().as_vec2();
                let inside = pos.x >= 0.0 && pos.y >= 0.0 && pos.x < viewport.x && pos.y < viewport.y;
                Pointer {
                    position: inside.then_some(pos),
                    just_pressed: m.just_pressed(MouseButton::Button1),
                    just_released: m.just_released(MouseButton::Button1),
                    scroll: m.scroll_delta().as_vec2(),
                    raw: pos,
                }
            });

            for root in entities {
                step::<A>(&world, root, viewport, &pointer, dt);
            }

            if deferred {
                world.defer_resume();
            }
        });
}

fn step<A: UiApp>(world: &World, root: Entity, viewport: Vec2, pointer: &Pointer, dt: f64) {
    let root_view = world.entity_from_id(root);

    // Taken out so no component borrow is held while nodes are created/destroyed
    let mut tree = root_view.get::<&mut UiTree>(std::mem::take);

    let mut msgs: Vec<A::Msg> = Vec::new();
    for (node, event) in tree.process_pointer(world, pointer) {
        let msg = world.entity_from_id(node).try_get::<&UiHandlers<A::Msg>>(|h| {
            let h = &h.0;
            match event {
                UiEvent::Enter => h.on_enter.clone(),
                UiEvent::Exit => h.on_exit.clone(),
                UiEvent::Press => h.on_press.clone(),
                UiEvent::Release => h.on_release.clone(),
                UiEvent::Click => h.on_click.clone(),
                UiEvent::Scroll(delta) => h.on_scroll.as_ref().map(|f| f(delta)),
                UiEvent::PressAt(e) => h.on_press_at.as_ref().map(|f| f(e)),
                UiEvent::Drag(e) => h.on_drag.as_ref().map(|f| f(e)),
            }
        });
        msgs.extend(msg.flatten());
    }

    let effects = root_view.get::<&mut UiRoot<A>>(|state| {
        state.time += dt;

        let mut queued = std::mem::take(&mut state.inbox);
        queued.extend(msgs);

        let mut effects = Vec::new();
        if !queued.is_empty() {
            state.view_dirty = true;
            let mut ctx = Ctx { effects: &mut effects, time: state.time };
            for msg in queued {
                state.app.update(msg, &mut ctx);
            }
        }
        effects
    });

    if !effects.is_empty() {
        let mut inbox = Inbox { msgs: Vec::new() };
        for effect in effects {
            A::perform(effect, world, &mut inbox);
        }
        if !inbox.msgs.is_empty() && world.is_alive(root) {
            root_view.get::<&mut UiRoot<A>>(|state| state.inbox.extend(inbox.msgs));
        }
    }

    if !world.is_alive(root) {
        return;
    }

    let view = root_view.get::<&mut UiRoot<A>>(|state| {
        std::mem::take(&mut state.view_dirty).then(|| state.app.view())
    });
    if let Some(view) = view {
        tree.reconcile(world, root, view);
    }

    tree.animate(world, dt as f32);

    world.try_get::<&RenderRegistry>(|registry| {
        let font = world.try_get::<&UiFonts>(|f| f.default).flatten();
        let res = Resources { registry: Some(registry), font };
        tree.layout(world, &res, viewport);
        tree.paint(world, &res);
    });

    root_view.get::<&mut UiTree>(|slot| *slot = tree);
}

pub trait UiWorldExt {
    /// Needed before the first `spawn_ui` if that happens inside a running system.
    fn register_ui_app<A: UiApp>(&self);

    fn spawn_ui<A: UiApp>(&self, app: A) -> EntityView<'_>;

    /// Delivered next tick.
    fn send_ui<A: UiApp>(&self, msg: A::Msg);
}

impl UiWorldExt for World {
    fn register_ui_app<A: UiApp>(&self) {
        register_app::<A>(self);
    }

    fn spawn_ui<A: UiApp>(&self, app: A) -> EntityView<'_> {
        register_app::<A>(self);
        self.entity()
            .set(UiRoot::new(app))
            .set(UiTree::default())
    }

    fn send_ui<A: UiApp>(&self, msg: A::Msg) {
        self.each::<&mut UiRoot<A>>(|root| root.send(msg.clone()));
    }
}

#[cfg(test)]
mod tests {
    use flecs_ecs::core::flecs::system::System;

    use super::*;
    use crate::{app::{input::Mouse, window::WindowSize}, ui::element::{column, text}};

    #[derive(Default)]
    struct Counter(u32);

    impl UiApp for Counter {
        type Msg = u32;
        type Effect = ();

        fn update(&mut self, msg: u32, _: &mut Ctx<()>) {
            self.0 += msg;
        }

        fn view(&self) -> Element<u32> {
            column().child(text(self.0.to_string()))
        }
    }

    #[derive(Default)]
    struct Label;

    impl UiApp for Label {
        type Msg = ();
        type Effect = ();

        fn update(&mut self, _: (), _: &mut Ctx<()>) {}

        fn view(&self) -> Element<()> {
            column().child(text("label")).child(text("second"))
        }
    }

    fn headless_world() -> (World, Entity) {
        let world = World::new();
        world.component::<WindowSize>().add_trait::<flecs::Singleton>();
        world.set(WindowSize { width: 800.0, height: 600.0, is_changed: false });
        world.component::<Mouse>().add_trait::<flecs::Singleton>();
        world.set(Mouse::default());

        let update = world.pipeline().with(System).with(Update).build().id();
        (world, update)
    }

    fn node_count(world: &World, root: Entity) -> usize {
        let tree_root = world.entity_from_id(root).get::<&UiTree>(|t| t.root());
        let Some(tree_root) = tree_root else { return 0 };
        1 + world.entity_from_id(tree_root).get::<&crate::ui::node::UiNode>(|n| n.children().len())
    }

    #[test]
    fn two_app_types_both_run() {
        let (world, update) = headless_world();

        let counter = world.spawn_ui(Counter::default()).id();
        let label = world.spawn_ui(Label).id();
        world.register_ui_app::<Counter>();

        world.run_pipeline_time(update, 0.016);

        assert_eq!(node_count(&world, counter), 2);
        assert_eq!(node_count(&world, label), 3);

        world.send_ui::<Counter>(5);
        world.run_pipeline_time(update, 0.016);

        let value = world.entity_from_id(counter).get::<&UiRoot<Counter>>(|r| r.model().0);
        assert_eq!(value, 5, "message must be applied exactly once");
    }
}
