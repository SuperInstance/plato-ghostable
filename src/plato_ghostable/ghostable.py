"""Ghostable — binary flag + cooldown wrapper for any tile/entity lifecycle."""
import time
from dataclasses import dataclass, field
from typing import Optional, Any
from enum import Enum

class GhostState(Enum):
    ALIVE = "alive"
    FADING = "fading"
    GHOST = "ghost"
    RESURRECTED = "resurrected"

@dataclass
class GhostableConfig:
    fade_duration: float = 3600.0  # seconds to fade before ghosting
    auto_ghost: bool = True
    max_resurrections: int = 10
    cooldown_after_ghost: float = 300.0  # cooldown before re-ghosting

@dataclass
class Ghostable:
    entity_id: str
    entity_type: str = "tile"
    state: GhostState = GhostState.ALIVE
    health: float = 1.0
    fade_started_at: float = 0.0
    ghosted_at: float = 0.0
    resurrected_at: float = 0.0
    resurrection_count: int = 0
    last_decay_tick: float = 0.0
    config: GhostableConfig = field(default_factory=GhostableConfig)
    payload: Any = None  # attached data

    def is_alive(self) -> bool:
        return self.state == GhostState.ALIVE or self.state == GhostState.RESURRECTED

    def is_ghost(self) -> bool:
        return self.state == GhostState.GHOST

    def is_fading(self) -> bool:
        return self.state == GhostState.FADING

    def start_fade(self) -> bool:
        if self.state != GhostState.ALIVE:
            return False
        self.state = GhostState.FADING
        self.fade_started_at = time.time()
        return True

    def decay(self, amount: float = 0.01) -> GhostState:
        if self.state not in (GhostState.ALIVE, GhostState.FADING):
            return self.state
        self.health = max(0.0, self.health - amount)
        self.last_decay_tick = time.time()
        if self.health <= 0.0 and self.config.auto_ghost:
            self.state = GhostState.GHOST
            self.ghosted_at = time.time()
        return self.state

    def resurrect(self, health: float = 1.0) -> bool:
        if self.state != GhostState.GHOST:
            return False
        if self.resurrection_count >= self.config.max_resurrections:
            return False
        cooldown_end = self.ghosted_at + self.config.cooldown_after_ghost
        if time.time() < cooldown_end:
            return False
        self.state = GhostState.RESURRECTED
        self.health = min(1.0, health)
        self.resurrected_at = time.time()
        self.resurrection_count += 1
        return True

    def kill(self):
        self.health = 0.0
        self.state = GhostState.GHOST
        self.ghosted_at = time.time()

    def boost(self, amount: float = 0.2) -> float:
        old = self.health
        self.health = min(1.0, self.health + amount)
        if self.state == GhostState.FADING and self.health > 0.5:
            self.state = GhostState.ALIVE
            self.fade_started_at = 0.0
        return self.health - old

    def time_as_ghost(self) -> float:
        if self.state != GhostState.GHOST:
            return 0.0
        return time.time() - self.ghosted_at

    def time_fading(self) -> float:
        if self.state != GhostState.FADING:
            return 0.0
        return time.time() - self.fade_started_at

    def to_dict(self) -> dict:
        return {"entity_id": self.entity_id, "entity_type": self.entity_type,
                "state": self.state.value, "health": round(self.health, 4),
                "resurrection_count": self.resurrection_count,
                "fade_started_at": self.fade_started_at, "ghosted_at": self.ghosted_at,
                "resurrected_at": self.resurrected_at}

class GhostableRegistry:
    def __init__(self):
        self._entities: dict[str, Ghostable] = {}

    def register(self, entity_id: str, entity_type: str = "tile",
                 config: GhostableConfig = None, payload: Any = None) -> Ghostable:
        g = Ghostable(entity_id=entity_id, entity_type=entity_type,
                     config=config or GhostableConfig(), payload=payload)
        self._entities[entity_id] = g
        return g

    def get(self, entity_id: str) -> Optional[Ghostable]:
        return self._entities.get(entity_id)

    def tick_all(self, decay: float = 0.01) -> list[str]:
        ghosted = []
        for eid, g in self._entities.items():
            prev = g.state
            g.decay(decay)
            if prev != GhostState.GHOST and g.state == GhostState.GHOST:
                ghosted.append(eid)
        return ghosted

    def ghosts(self) -> list[Ghostable]:
        return [g for g in self._entities.values() if g.state == GhostState.GHOST]

    def alive(self) -> list[Ghostable]:
        return [g for g in self._entities.values() if g.is_alive()]

    def fading(self) -> list[Ghostable]:
        return [g for g in self._entities.values() if g.state == GhostState.FADING]

    def resurrect(self, entity_id: str, health: float = 1.0) -> bool:
        g = self._entities.get(entity_id)
        return g.resurrect(health) if g else False

    @property
    def stats(self) -> dict:
        states = {}
        for g in self._entities.values():
            states[g.state.value] = states.get(g.state.value, 0) + 1
        return {"total": len(self._entities), "states": states}
