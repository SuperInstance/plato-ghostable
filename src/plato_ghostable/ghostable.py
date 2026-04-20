"""Ghostable lifecycle management."""

import time
from dataclasses import dataclass, field
from enum import Enum

class GhostState(Enum):
    ACTIVE = "active"
    GHOST = "ghost"
    RESURRECTED = "resurrected"

@dataclass
class Ghostable:
    id: str
    content: str = ""
    decay_rate: float = 0.96
    ghost_threshold: float = 0.05
    _health: float = 1.0
    _state: GhostState = GhostState.ACTIVE
    _use_count: int = 0
    _ghosted_at: float = 0.0
    _resurrected_at: float = 0.0

    def access(self):
        self._use_count += 1
        self._health = min(self._health + 0.1, 1.0)
        if self._state == GhostState.GHOST:
            self._state = GhostState.RESURRECTED
            self._resurrected_at = time.time()

    def decay(self):
        self._health *= self.decay_rate
        if self._health < self.ghost_threshold and self._state == GhostState.ACTIVE:
            self._state = GhostState.GHOST
            self._ghosted_at = time.time()

    def is_ghost(self) -> bool:
        return self._state == GhostState.GHOST

    def is_active(self) -> bool:
        return self._state == GhostState.ACTIVE

    def resurrect(self, boost: float = 0.5):
        self._health = min(self._health + boost, 1.0)
        self._state = GhostState.RESURRECTED
        self._resurrected_at = time.time()

    def kill(self):
        self._health = 0.0
        self._state = GhostState.GHOST
        self._ghosted_at = time.time()

    @property
    def health(self) -> float:
        return self._health

    @property
    def state(self) -> GhostState:
        return self._state

    @property
    def use_count(self) -> int:
        return self._use_count

    @property
    def stats(self) -> dict:
        return {"id": self.id, "state": self._state.value, "health": self._health,
                "use_count": self._use_count, "ghosted_at": self._ghosted_at,
                "resurrected_at": self._resurrected_at}
