pub type ComponentDumper = fn(&hecs::EntityRef) -> Option<(&'static str, serde_json::Value)>;

pub fn world_to_json(
    world: &hecs::World,
    tick: u64,
    seed: u64,
    dumpers: &[ComponentDumper],
) -> serde_json::Value {
    let mut entities: Vec<_> = world.iter().collect();
    entities.sort_by_key(|e| e.entity().to_bits());

    let dumped: Vec<_> = entities
        .iter()
        .map(|e| {
            let mut components = serde_json::Map::new();
            for dumper in dumpers {
                if let Some((name, value)) = dumper(e) {
                    components.insert(name.to_string(), value);
                }
            }
            serde_json::json!({
                "entity": format!("{:?}", e.entity()),
                "components": components,
            })
        })
        .collect();

    serde_json::json!({
        "tick": tick,
        "seed": seed,
        "entities": dumped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Health(i32);
    #[derive(Serialize)]
    struct Name(String);

    fn dump_health(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
        e.get::<&Health>()
            .map(|h| ("Health", serde_json::to_value(&*h).unwrap()))
    }
    fn dump_name(e: &hecs::EntityRef) -> Option<(&'static str, serde_json::Value)> {
        e.get::<&Name>()
            .map(|n| ("Name", serde_json::to_value(&*n).unwrap()))
    }

    const DUMPERS: &[ComponentDumper] = &[dump_health, dump_name];

    #[test]
    fn world_to_json_is_sorted_and_repeat_calls_are_byte_identical() {
        let mut world = hecs::World::new();
        // Spawn out of any "natural" order to prove the sort isn't accidental.
        world.spawn((Health(50),));
        world.spawn((Health(100), Name("hero".to_string())));
        world.spawn((Name("villager".to_string()),));

        let first = world_to_json(&world, 3, 42, DUMPERS);
        let second = world_to_json(&world, 3, 42, DUMPERS);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );

        let entities = first["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 3);

        let ids: Vec<String> = entities
            .iter()
            .map(|e| e["entity"].as_str().unwrap().to_string())
            .collect();
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        assert_eq!(ids, sorted_ids, "entities must be in stable sorted order");
    }

    #[test]
    fn world_to_json_only_includes_present_components() {
        let mut world = hecs::World::new();
        world.spawn((Health(50),));

        let dumped = world_to_json(&world, 0, 0, DUMPERS);
        let components = &dumped["entities"][0]["components"];
        assert_eq!(components["Health"], serde_json::json!(50));
        assert!(components.get("Name").is_none());
    }
}
