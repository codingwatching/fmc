use super::components::{F64GlobalTransform, F64Transform};
use bevy::ecs::prelude::{Changed, Entity, Query, With, Without};
use bevy::hierarchy::{Children, Parent};

/// Update [`GlobalTransform`] component of entities that aren't in the hierarchy
///
/// Third party plugins should ensure that this is used in concert with [`propagate_transforms`].
pub fn sync_simple_transforms(
    mut query: Query<
        (&F64Transform, &mut F64GlobalTransform),
        (Changed<F64Transform>, Without<Parent>, Without<Children>),
    >,
) {
    query
        .par_iter_mut()
        .for_each_mut(|(transform, mut global_transform)| {
            *global_transform = F64GlobalTransform::from(*transform);
        });
}

/// Update [`GlobalTransform`] component of entities based on entity hierarchy and
/// [`Transform`] component.
pub fn propagate_transforms(
    mut root_query: Query<
        (
            Option<(&Children, Changed<Children>)>,
            &F64Transform,
            Changed<F64Transform>,
            &mut F64GlobalTransform,
            Entity,
        ),
        Without<Parent>,
    >,
    mut transform_query: Query<(
        &F64Transform,
        Changed<F64Transform>,
        &mut F64GlobalTransform,
        &Parent,
    )>,
    children_query: Query<(&Children, Changed<Children>), (With<Parent>, With<F64GlobalTransform>)>,
) {
    for (children, transform, transform_changed, mut global_transform, entity) in
        root_query.iter_mut()
    {
        let mut changed = transform_changed;
        if transform_changed {
            *global_transform = F64GlobalTransform::from(*transform);
        }

        if let Some((children, changed_children)) = children {
            // If our `Children` has changed, we need to recalculate everything below us
            changed |= changed_children;
            for child in children {
                let _ = propagate_recursive(
                    &global_transform,
                    &mut transform_query,
                    &children_query,
                    *child,
                    entity,
                    changed,
                );
            }
        }
    }
}

fn propagate_recursive(
    parent: &F64GlobalTransform,
    transform_query: &mut Query<(
        &F64Transform,
        Changed<F64Transform>,
        &mut F64GlobalTransform,
        &Parent,
    )>,
    children_query: &Query<
        (&Children, Changed<Children>),
        (With<Parent>, With<F64GlobalTransform>),
    >,
    entity: Entity,
    expected_parent: Entity,
    mut changed: bool,
    // We use a result here to use the `?` operator. Ideally we'd use a try block instead
) -> Result<(), ()> {
    let global_matrix = {
        let (transform, transform_changed, mut global_transform, child_parent) =
            transform_query.get_mut(entity).map_err(drop)?;
        // Note that for parallelising, this check cannot occur here, since there is an `&mut GlobalTransform` (in global_transform)
        assert_eq!(
            child_parent.get(), expected_parent,
            "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
        );
        changed |= transform_changed;
        if changed {
            *global_transform = parent.mul_transform(*transform);
        }
        *global_transform
    };

    let (children, changed_children) = children_query.get(entity).map_err(drop)?;
    // If our `Children` has changed, we need to recalculate everything below us
    changed |= changed_children;
    for child in children {
        let _ = propagate_recursive(
            &global_matrix,
            transform_query,
            children_query,
            *child,
            entity,
            changed,
        );
    }
    Ok(())
}
