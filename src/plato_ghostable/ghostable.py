"""Ghostable — ghost tile lifecycle with afterlife reef, resurrection, haunting, and decay tracking."""
import time
import math
from dataclasses import dataclass, field
from typing import Optional, Callable
from collections import defaultdict
from enum import Enum

class TileState(Enum):
    ALIVE = "alive"
    FADING = "fading"
    GHOST = "ghost"
    HAUNTING = "haunting"
    RESURRECTED = "resurrected"
    AFTERLIFE = "afterlife"
    EXPIRED = "expired"

class ResurrectionCondition(Enum):
    MANUAL = "manual"
    REFERENCE = "reference"    # another tile references this one
    IMPORTANCE = "importance"  # was once high-importance
    SCHEDULED = "scheduled"    # scheduled resurrection at time
    THRESHOLD = "threshold"    # room ghost count exceeds threshold

@dataclass
class GhostTile:
    id: str
    content: str
    room: str = ""
    state: TileState = TileState.ALIVE
    health: float = 1.0        # 1.0 = full health, 0.0 = ghost
    ghost_threshold: float = 0.1
    original_confidence: float = 0.5
    importance: float = 0.5
    created_at: float = field(default_factory=time.time)
    ghosted_at: float = 0.0
    resurrected_at: float = 0.0
    resurrection_count: int = 0
    last_accessed: float = 0.0
    references: int = 0         # how many tiles reference this
    decay_rate: float = 0.01    # health loss per hour
    haunt_boost: float = 0.0    # health boost from haunting
    metadata: dict = field(default_factory=dict)

@dataclass
class AfterlifeReef:
    room: str
    ghosts: list[str] = field(default_factory=list)
    capacity: int = 1000
    oldest_ghost: float = 0.0

@dataclass
class DecayEvent:
    tile_id: str
    room: str
    from_state: TileState
    to_state: TileState
    health: float
    timestamp: float = field(default_factory=time.time)
    reason: str = ""

class GhostableSystem:
    def __init__(self, ghost_threshold: float = 0.1, decay_rate: float = 0.01,
                 afterlife_capacity: int = 1000):
        self.ghost_threshold = ghost_threshold
        self.default_decay_rate = decay_rate
        self._tiles: dict[str, GhostTile] = {}
        self._afterlife: dict[str, AfterlifeReef] = {}  # room → reef
        self._decay_log: list[ DecayEvent] = []
        self._resurrection_rules: list[dict] = []

    def register(self, tile_id: str, content: str, room: str = "", confidence: float = 0.5,
                importance: float = 0.5) -> GhostTile:
        tile = GhostTile(id=tile_id, content=content, room=room,
                        original_confidence=confidence, importance=importance,
                        ghost_threshold=self.ghost_threshold,
                        decay_rate=self.default_decay_rate)
        self._tiles[tile_id] = tile
        return tile

    def access(self, tile_id: str) -> Optional[GhostTile]:
        tile = self._tiles.get(tile_id)
        if tile and tile.state in (TileState.ALIVE, TileState.FADING):
            tile.last_accessed = time.time()
            tile.health = min(1.0, tile.health + 0.05)  # access boosts health
        return tile

    def add_reference(self, tile_id: str):
        tile = self._tiles.get(tile_id)
        if tile:
            tile.references += 1
            tile.health = min(1.0, tile.health + 0.02)  # reference boosts health

    def remove_reference(self, tile_id: str):
        tile = self._tiles.get(tile_id)
        if tile:
            tile.references = max(0, tile.references - 1)

    def add_resurrection_rule(self, condition: str, check_fn: Callable):
        self._resurrection_rules.append({"condition": ResurrectionCondition(condition), "fn": check_fn})

    def tick(self, room: str = "") -> list[DecayEvent]:
        """Process one decay tick. Returns list of state transitions."""
        events = []
        now = time.time()
        tiles = [t for t in self._tiles.values() if not room or t.room == room]
        for tile in tiles:
            if tile.state not in (TileState.ALIVE, TileState.FADING, TileState.HAUNTING):
                continue
            hours_since_access = (now - tile.last_accessed) / 3600 if tile.last_accessed > 0 else (now - tile.created_at) / 3600
            decay = tile.decay_rate * hours_since_access
            tile.health = max(0.0, tile.health - decay + tile.haunt_boost)
            tile.haunt_boost = max(0.0, tile.haunt_boost - 0.005)  # haunt boost fades
            # State transitions
            old_state = tile.state
            if tile.state == TileState.ALIVE and tile.health < 0.3:
                tile.state = TileState.FADING
            if tile.health <= tile.ghost_threshold and tile.state in (TileState.ALIVE, TileState.FADING):
                self._send_to_afterlife(tile)
                tile.state = TileState.GHOST
                tile.ghosted_at = now
                events.append(DecayEvent(tile.id, tile.room, old_state, TileState.GHOST,
                                        tile.health, reason="health below threshold"))
            if tile.state == TileState.HAUNTING and tile.health > 0.5:
                tile.state = TileState.RESURRECTED
                tile.resurrected_at = now
                tile.resurrection_count += 1
                events.append(DecayEvent(tile.id, tile.room, old_state, TileState.RESURRECTED,
                                        tile.health, reason="haunting restored health"))
        # Check resurrection rules
        events.extend(self._check_resurrections(room))
        self._decay_log.extend(events)
        if len(self._decay_log) > 10000:
            self._decay_log = self._decay_log[-10000:]
        return events

    def haunt(self, tile_id: str, boost: float = 0.2) -> bool:
        tile = self._tiles.get(tile_id)
        if not tile or tile.state != TileState.GHOST:
            return False
        tile.state = TileState.HAUNTING
        tile.haunt_boost = boost
        return True

    def resurrect(self, tile_id: str, health: float = 0.8) -> bool:
        tile = self._tiles.get(tile_id)
        if not tile or tile.state not in (TileState.GHOST, TileState.AFTERLIFE):
            return False
        tile.state = TileState.RESURRECTED
        tile.health = health
        tile.resurrected_at = time.time()
        tile.resurrection_count += 1
        self._remove_from_afterlife(tile)
        return True

    def expire(self, tile_id: str) -> bool:
        tile = self._tiles.get(tile_id)
        if not tile:
            return False
        tile.state = TileState.EXPIRED
        tile.health = 0.0
        return True

    def ghosts(self, room: str = "") -> list[GhostTile]:
        tiles = [t for t in self._tiles.values() if t.state in (TileState.GHOST, TileState.HAUNTING)]
        if room:
            tiles = [t for t in tiles if t.room == room]
        return tiles

    def afterlife(self, room: str) -> AfterlifeReef:
        if room not in self._afterlife:
            self._afterlife[room] = AfterlifeReef(room=room)
        return self._afterlife[room]

    def _send_to_afterlife(self, tile: GhostTile):
        reef = self.afterlife(tile.room)
        if len(reef.ghosts) >= reef.capacity:
            reef.ghosts.pop(0)  # remove oldest
        reef.ghosts.append(tile.id)
        reef.oldest_ghost = time.time()
        tile.state = TileState.AFTERLIFE

    def _remove_from_afterlife(self, tile: GhostTile):
        reef = self._afterlife.get(tile.room)
        if reef and tile.id in reef.ghosts:
            reef.ghosts.remove(tile.id)

    def _check_resurrections(self, room: str) -> list[DecayEvent]:
        events = []
        for rule in self._resurrection_rules:
            try:
                results = rule["fn"](self._tiles, room)
                for tile_id in results:
                    tile = self._tiles.get(tile_id)
                    if tile and tile.state == TileState.GHOST:
                        old = tile.state
                        tile.state = TileState.RESURRECTED
                        tile.health = 0.6
                        tile.resurrected_at = time.time()
                        tile.resurrection_count += 1
                        events.append(DecayEvent(tile_id, tile.room, old,
                                                TileState.RESURRECTED, tile.health,
                                                reason=rule["condition"].value))
            except:
                pass
        return events

    def decay_log(self, limit: int = 50) -> list[DecayEvent]:
        return self._decay_log[-limit:]

    @property
    def stats(self) -> dict:
        states = defaultdict(int)
        for t in self._tiles.values():
            states[t.state.value] += 1
        return {"tiles": len(self._tiles), "states": dict(states),
                "afterlife_rooms": len(self._afterlife),
                "resurrection_rules": len(self._resurrection_rules),
                "decay_events": len(self._decay_log)}
