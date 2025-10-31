//! Bevy plugin to send messages after delay.
//!
//! ```
//! use bevy::prelude::*;
//! use bevy_write_after::{MessagePool, WriteAfterPlugin};
//!
//! #[derive(Message)]
//! struct MyMessage;
//!
//! #[derive(Component)]
//! struct MyPool;
//!
//! fn my_main() {
//!     App::new()
//!         .add_message::<MyMessage>()
//!         .add_plugins(WriteAfterPlugin)
//!         .add_systems(Startup, setup)
//!         .add_systems(Startup, some_system)
//!         .add_systems(Update, on_my_message.run_if(on_message::<MyMessage>));
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn((
//!         MessagePool::default(),
//!         MyPool,
//!     ));
//! }
//!
//! fn some_system(mut pool: Single<&mut MessagePool, With<MyPool>>) {
//!     pool.write_after(MyMessage, 1.0);
//! }
//!
//! fn on_my_message() {
//!     println!("received my message");
//! }
//!
//! ```
use bevy::prelude::*;

pub struct WriteAfterPlugin;

impl Plugin for WriteAfterPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_message::<MessagePoolEmptied>()
            .add_systems(Startup, spawn_global_message_pool)
            .add_systems(Update, process_messages);
    }
}

struct QueuedMessage {
    timer: Timer,
    write_fn: Box<dyn FnOnce(&mut Commands) + Send + Sync + 'static>,
}

type CustomEmptiedMessage = Box<dyn Fn(&mut Commands, Entity) + Send + Sync + 'static>;

/// Message sent when the pool is empty.
#[derive(Message)]
pub struct MessagePoolEmptied(pub Entity);

/// Global message pool.
#[derive(Component)]
pub struct GlobalMessagePool;

#[derive(Component, Default)]
pub struct MessagePool {
    messages: Vec<QueuedMessage>,
    when_emptied: Option<CustomEmptiedMessage>,
}

impl MessagePool {
    pub fn write_after<M: Message + Send + Sync + 'static>(&mut self, message: M, delay: f32) {
        let timer = Timer::from_seconds(delay, TimerMode::Once);

        let write_fn = Box::new(move |commands: &mut Commands| {
            commands.queue(move |world: &mut World| {
                world.resource_mut::<Messages<M>>().write(message);
            });
        });

        self.messages.push(QueuedMessage { timer, write_fn });
    }

    pub fn write_when_empty<M: Message + Send + Sync + Clone + 'static>(&mut self, message: M) {
        let write_fn = Box::new(move |commands: &mut Commands, emptied: Entity| {
            let message = message.clone();
            commands.queue(move |world: &mut World| {
                world.resource_mut::<Messages<MessagePoolEmptied>>().write(MessagePoolEmptied(emptied));
                world.resource_mut::<Messages<M>>().write(message);
            });
        });

        self.when_emptied = Some(write_fn);
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

fn spawn_global_message_pool(
    mut commands: Commands
) {
    commands.spawn((
        MessagePool::default(),
        GlobalMessagePool,
    ));
}

fn process_messages(
    mut commands: Commands,
    time: Res<Time>,
    query: Query<(Entity, &mut MessagePool)>,
) {
    for (entity, mut pool) in query {
        let mut finished = Vec::new();

        for (i, message) in pool.messages.iter_mut().enumerate() {
            message.timer.tick(time.delta());
            if message.timer.is_finished() {
                finished.push(i);
            }
        }

        for i in finished.into_iter().rev() {
            let message = pool.messages.remove(i);
            (message.write_fn)(&mut commands);
            if let Some(ref when_empty) = pool.when_emptied && pool.messages.is_empty() {
                (when_empty)(&mut commands, entity);
            }
        }
        
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::prelude::*;
    use super::*;

    #[derive(Message, Debug, PartialEq)]
    struct TestMessage(&'static str);

    #[test]
    fn test_message_after() {
        fn add_message_hello(
            mut after: Single<&mut MessagePool, Added<MessagePool>>,
        ) {
            after.write_after(TestMessage("hello"), 1.0);
        }
        
        fn add_message_hello2(
            mut after: Single<&mut MessagePool, Added<MessagePool>>,
        ) {
            after.write_after(TestMessage("hello2"), 2.0);
        }

        let mut app = App::new();
        app.add_message::<TestMessage>();
        app.init_resource::<Time>();
        app.add_plugins(WriteAfterPlugin);
        app.add_systems(Update, add_message_hello);
        app.add_systems(Update, add_message_hello2);
        app.update();

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.5));
        app.update();
        assert!(app.world_mut().resource_mut::<Messages<TestMessage>>().is_empty(), "should be empty");


        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(1.0));
        app.update();
        assert!(!app.world_mut().resource_mut::<Messages<TestMessage>>().is_empty(), "should not be empty");
        assert_eq!(app.world_mut().resource_mut::<Messages<TestMessage>>().drain().collect::<Vec<_>>(), vec![TestMessage("hello")]);
        assert!(app.world_mut().resource_mut::<Messages<TestMessage>>().is_empty(), "should be empty");

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(1.0));
        app.update();
        assert!(!app.world_mut().resource_mut::<Messages<TestMessage>>().is_empty(), "should not be empty");
        assert_eq!(app.world_mut().resource_mut::<Messages<TestMessage>>().drain().collect::<Vec<_>>(), vec![TestMessage("hello2")]);
    }
}

