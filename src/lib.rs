use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Lifecycle state of a tile or agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileState {
    Alive,
    Fading,
    Ghost,
    Haunting,
    Resurrected,
    Afterlife,
    Expired,
}

impl TileState {
    /// Return the snake_case string value used in the Python original.
    pub fn value(&self) -> &'static str {
        match self {
            TileState::Alive => "alive",
            TileState::Fading => "fading",
            TileState::Ghost => "ghost",
            TileState::Haunting => "haunting",
            TileState::Resurrected => "resurrected",
            TileState::Afterlife => "afterlife",
            TileState::Expired => "expired",
        }
    }
}

/// Alias kept for API compatibility with the Python package.
pub type GhostState = TileState;

/// Why a tile was resurrected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResurrectionCondition {
    Manual,
    Reference,
    Importance,
    Scheduled,
    Threshold,
}

impl ResurrectionCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResurrectionCondition::Manual => "manual",
            ResurrectionCondition::Reference => "reference",
            ResurrectionCondition::Importance => "importance",
            ResurrectionCondition::Scheduled => "scheduled",
            ResurrectionCondition::Threshold => "threshold",
        }
    }
}

impl TryFrom<&str> for ResurrectionCondition {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "manual" => Ok(ResurrectionCondition::Manual),
            "reference" => Ok(ResurrectionCondition::Reference),
            "importance" => Ok(ResurrectionCondition::Importance),
            "scheduled" => Ok(ResurrectionCondition::Scheduled),
            "threshold" => Ok(ResurrectionCondition::Threshold),
            _ => Err(format!("unknown condition: {value}")),
        }
    }
}

/// A single tile / agent tracked by the ghostable system.
#[derive(Debug, Clone)]
pub struct GhostTile {
    pub id: String,
    pub content: String,
    pub room: String,
    pub state: TileState,
    pub health: f64,
    pub ghost_threshold: f64,
    pub original_confidence: f64,
    pub importance: f64,
    pub created_at: f64,
    pub ghosted_at: f64,
    pub resurrected_at: f64,
    pub resurrection_count: u32,
    pub last_accessed: f64,
    pub references: u32,
    pub decay_rate: f64,
    pub haunt_boost: f64,
    pub metadata: HashMap<String, String>,
}

impl GhostTile {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        let now = now();
        GhostTile {
            id: id.into(),
            content: content.into(),
            room: String::new(),
            state: TileState::Alive,
            health: 1.0,
            ghost_threshold: 0.1,
            original_confidence: 0.5,
            importance: 0.5,
            created_at: now,
            ghosted_at: 0.0,
            resurrected_at: 0.0,
            resurrection_count: 0,
            last_accessed: 0.0,
            references: 0,
            decay_rate: 0.01,
            haunt_boost: 0.0,
            metadata: HashMap::new(),
        }
    }
}

/// Trait exposed for API compatibility with the Python `Ghostable` export.
pub trait Ghostable {
    fn id(&self) -> &str;
    fn content(&self) -> &str;
    fn state(&self) -> TileState;
    fn health(&self) -> f64;
    fn is_ghost(&self) -> bool;
    fn is_alive(&self) -> bool;
    fn can_resurrect(&self) -> bool;
}

impl Ghostable for GhostTile {
    fn id(&self) -> &str {
        &self.id
    }

    fn content(&self) -> &str {
        &self.content
    }

    fn state(&self) -> TileState {
        self.state
    }

    fn health(&self) -> f64 {
        self.health
    }

    fn is_ghost(&self) -> bool {
        matches!(self.state, TileState::Ghost | TileState::Haunting)
    }

    fn is_alive(&self) -> bool {
        matches!(self.state, TileState::Alive | TileState::Fading)
    }

    fn can_resurrect(&self) -> bool {
        matches!(self.state, TileState::Ghost | TileState::Afterlife)
    }
}

/// Per-room afterlife storage.
#[derive(Debug, Clone)]
pub struct AfterlifeReef {
    pub room: String,
    pub ghosts: Vec<String>,
    pub capacity: usize,
    pub oldest_ghost: f64,
}

impl AfterlifeReef {
    pub fn new(room: impl Into<String>) -> Self {
        AfterlifeReef {
            room: room.into(),
            ghosts: Vec::new(),
            capacity: 1000,
            oldest_ghost: 0.0,
        }
    }
}

/// Record of a state transition.
#[derive(Debug, Clone)]
pub struct DecayEvent {
    pub tile_id: String,
    pub room: String,
    pub from_state: TileState,
    pub to_state: TileState,
    pub health: f64,
    pub timestamp: f64,
    pub reason: String,
}

/// Summary statistics produced by [`GhostableSystem::stats`].
#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub tiles: usize,
    pub states: HashMap<String, usize>,
    pub afterlife_rooms: usize,
    pub resurrection_rules: usize,
    pub decay_events: usize,
}

type ResurrectionFn = Box<dyn Fn(&HashMap<String, GhostTile>, &str) -> Vec<String> + Send + Sync>;

/// Core system that manages ghostable tiles.
pub struct GhostableSystem {
    pub ghost_threshold: f64,
    pub default_decay_rate: f64,
    tiles: HashMap<String, GhostTile>,
    afterlife: HashMap<String, AfterlifeReef>,
    decay_log: Vec<DecayEvent>,
    resurrection_rules: Vec<(ResurrectionCondition, ResurrectionFn)>,
}

impl GhostableSystem {
    pub fn new(ghost_threshold: f64, decay_rate: f64, _afterlife_capacity: usize) -> Self {
        GhostableSystem {
            ghost_threshold,
            default_decay_rate: decay_rate,
            tiles: HashMap::new(),
            afterlife: HashMap::new(),
            decay_log: Vec::new(),
            resurrection_rules: Vec::new(),
        }
    }

    pub fn register(
        &mut self,
        tile_id: impl Into<String>,
        content: impl Into<String>,
        room: impl Into<String>,
        confidence: f64,
        importance: f64,
    ) -> GhostTile {
        let mut tile = GhostTile::new(tile_id, content);
        tile.room = room.into();
        tile.original_confidence = confidence;
        tile.importance = importance;
        tile.ghost_threshold = self.ghost_threshold;
        tile.decay_rate = self.default_decay_rate;
        self.tiles.insert(tile.id.clone(), tile.clone());
        tile
    }

    pub fn access(&mut self, tile_id: &str) -> Option<GhostTile> {
        let tile = self.tiles.get_mut(tile_id)?;
        if matches!(tile.state, TileState::Alive | TileState::Fading) {
            tile.last_accessed = now();
            tile.health = (tile.health + 0.05).min(1.0);
        }
        Some(tile.clone())
    }

    pub fn add_reference(&mut self, tile_id: &str) {
        if let Some(tile) = self.tiles.get_mut(tile_id) {
            tile.references += 1;
            tile.health = (tile.health + 0.02).min(1.0);
        }
    }

    pub fn remove_reference(&mut self, tile_id: &str) {
        if let Some(tile) = self.tiles.get_mut(tile_id) {
            tile.references = tile.references.saturating_sub(1);
        }
    }

    pub fn add_resurrection_rule<F>(&mut self, condition: &str, check_fn: F)
    where
        F: Fn(&HashMap<String, GhostTile>, &str) -> Vec<String> + Send + Sync + 'static,
    {
        let condition = ResurrectionCondition::try_from(condition)
            .expect("invalid resurrection condition");
        self.resurrection_rules.push((condition, Box::new(check_fn)));
    }

    pub fn tick(&mut self, room: &str) -> Vec<DecayEvent> {
        let mut events = Vec::new();
        let now_ts = now();

        let tile_ids: Vec<String> = self
            .tiles
            .values()
            .filter(|t| room.is_empty() || t.room == room)
            .map(|t| t.id.clone())
            .collect();

        for id in tile_ids {
            let tile = match self.tiles.get_mut(&id) {
                Some(t) => t,
                None => continue,
            };

            if !matches!(tile.state, TileState::Alive | TileState::Fading | TileState::Haunting) {
                continue;
            }

            let hours_since_access = if tile.last_accessed > 0.0 {
                (now_ts - tile.last_accessed) / 3600.0
            } else {
                (now_ts - tile.created_at) / 3600.0
            };

            let decay = tile.decay_rate * hours_since_access;
            tile.health = (tile.health - decay + tile.haunt_boost).max(0.0);
            tile.haunt_boost = (tile.haunt_boost - 0.005).max(0.0);

            let old_state = tile.state;

            if tile.state == TileState::Alive && tile.health < 0.3 {
                tile.state = TileState::Fading;
            }

            if tile.health <= tile.ghost_threshold
                && matches!(tile.state, TileState::Alive | TileState::Fading)
            {
                // Inline send_to_afterlife to keep borrow checker happy.
                let room_name = tile.room.clone();
                let reef = self
                    .afterlife
                    .entry(room_name.clone())
                    .or_insert_with(|| AfterlifeReef::new(room_name));
                if reef.ghosts.len() >= reef.capacity {
                    reef.ghosts.remove(0);
                }
                reef.ghosts.push(tile.id.clone());
                reef.oldest_ghost = now_ts;
                tile.state = TileState::Afterlife;

                tile.state = TileState::Ghost;
                tile.ghosted_at = now_ts;
                events.push(DecayEvent {
                    tile_id: id.clone(),
                    room: tile.room.clone(),
                    from_state: old_state,
                    to_state: TileState::Ghost,
                    health: tile.health,
                    timestamp: now_ts,
                    reason: "health below threshold".to_string(),
                });
            }

            if tile.state == TileState::Haunting && tile.health > 0.5 {
                tile.state = TileState::Resurrected;
                tile.resurrected_at = now_ts;
                tile.resurrection_count += 1;
                events.push(DecayEvent {
                    tile_id: id.clone(),
                    room: tile.room.clone(),
                    from_state: old_state,
                    to_state: TileState::Resurrected,
                    health: tile.health,
                    timestamp: now_ts,
                    reason: "haunting restored health".to_string(),
                });
            }
        }

        events.extend(self.check_resurrections(room));
        self.decay_log.extend(events.clone());
        if self.decay_log.len() > 10000 {
            let split = self.decay_log.len() - 10000;
            self.decay_log = self.decay_log.split_off(split);
        }
        events
    }

    pub fn haunt(&mut self, tile_id: &str, boost: f64) -> bool {
        if let Some(tile) = self.tiles.get_mut(tile_id) {
            if tile.state == TileState::Ghost {
                tile.state = TileState::Haunting;
                tile.haunt_boost = boost;
                return true;
            }
        }
        false
    }

    pub fn resurrect(&mut self, tile_id: &str, health: f64) -> bool {
        if let Some(tile) = self.tiles.get_mut(tile_id) {
            if matches!(tile.state, TileState::Ghost | TileState::Afterlife) {
                // Inline remove_from_afterlife.
                if let Some(reef) = self.afterlife.get_mut(&tile.room) {
                    if let Some(pos) = reef.ghosts.iter().position(|id| id == tile_id) {
                        reef.ghosts.remove(pos);
                    }
                }

                tile.state = TileState::Resurrected;
                tile.health = health;
                tile.resurrected_at = now();
                tile.resurrection_count += 1;
                return true;
            }
        }
        false
    }

    pub fn expire(&mut self, tile_id: &str) -> bool {
        if let Some(tile) = self.tiles.get_mut(tile_id) {
            tile.state = TileState::Expired;
            tile.health = 0.0;
            return true;
        }
        false
    }

    pub fn ghosts(&self, room: &str) -> Vec<GhostTile> {
        self.tiles
            .values()
            .filter(|t| matches!(t.state, TileState::Ghost | TileState::Haunting))
            .filter(|t| room.is_empty() || t.room == room)
            .cloned()
            .collect()
    }

    pub fn afterlife(&mut self, room: &str) -> &AfterlifeReef {
        self.afterlife
            .entry(room.to_string())
            .or_insert_with(|| AfterlifeReef::new(room))
    }

    fn check_resurrections(&mut self, room: &str) -> Vec<DecayEvent> {
        let mut events = Vec::new();

        let results: Vec<(ResurrectionCondition, Vec<String>)> = self
            .resurrection_rules
            .iter()
            .map(|(cond, f)| (*cond, f(&self.tiles, room)))
            .collect();

        for (condition, tile_ids) in results {
            for tile_id in tile_ids {
                if let Some(tile) = self.tiles.get_mut(&tile_id) {
                    if tile.state == TileState::Ghost {
                        let old = tile.state;
                        tile.state = TileState::Resurrected;
                        tile.health = 0.6;
                        tile.resurrected_at = now();
                        tile.resurrection_count += 1;
                        events.push(DecayEvent {
                            tile_id: tile.id.clone(),
                            room: tile.room.clone(),
                            from_state: old,
                            to_state: TileState::Resurrected,
                            health: tile.health,
                            timestamp: now(),
                            reason: condition.as_str().to_string(),
                        });
                    }
                }
            }
        }
        events
    }

    pub fn decay_log(&self, limit: usize) -> Vec<DecayEvent> {
        self.decay_log.iter().rev().take(limit).rev().cloned().collect()
    }

    pub fn stats(&self) -> Stats {
        let mut states = HashMap::new();
        for t in self.tiles.values() {
            *states.entry(t.state.value().to_string()).or_insert(0) += 1;
        }
        Stats {
            tiles: self.tiles.len(),
            states,
            afterlife_rooms: self.afterlife.len(),
            resurrection_rules: self.resurrection_rules.len(),
            decay_events: self.decay_log.len(),
        }
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_access() {
        let mut sys = GhostableSystem::new(0.1, 0.01, 1000);
        let tile = sys.register("t1", "hello", "room-a", 0.5, 0.5);
        assert_eq!(tile.id, "t1");
        assert!(sys.access("t1").is_some());
        assert!(sys.access("missing").is_none());
    }

    #[test]
    fn ghost_lifecycle() {
        let mut sys = GhostableSystem::new(0.1, 0.01, 1000);
        sys.register("t1", "hello", "room-a", 0.5, 0.5);
        // Force health low so tick ghosts it.
        if let Some(t) = sys.tiles.get_mut("t1") {
            t.health = 0.05;
            t.last_accessed = now() - 7200.0; // 2 hours ago
        }
        let events = sys.tick("room-a");
        assert!(!events.is_empty());
        assert_eq!(sys.ghosts("room-a").len(), 1);
        assert!(sys.resurrect("t1", 0.8));
        assert_eq!(sys.ghosts("room-a").len(), 0);
    }
}
