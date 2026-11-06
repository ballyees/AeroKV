from __future__ import annotations

import asyncio
import json
import secrets
import string
from typing import Any
from uuid import uuid4

try:
    from channels.exceptions import ChannelFull  # type: ignore
except ImportError:
    class ChannelFull(Exception):  # type: ignore
        """Raised when a channel exceeds its maximum message capacity."""

try:
    import msgpack  # type: ignore
    def _serialize(msg: dict) -> bytes:
        return msgpack.packb(msg, use_bin_type=True)
    def _deserialize(data: bytes) -> dict:
        return msgpack.unpackb(data, raw=False)
except ImportError:
    def _serialize(msg: dict) -> bytes:
        return json.dumps(msg, separators=(",", ":")).encode("utf-8")
    def _deserialize(data: bytes) -> dict:
        return json.loads(data.decode("utf-8"))


class AeroChannelLayer:
    """
    Native high-performance ASGI Channel Layer backed by AeroKV.
    Fully implements the Django Channels Layer specification:
    https://channels.readthedocs.io/en/stable/topics/channel_layers.html
    """

    extensions = ["groups", "flush"]

    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 16379,
        hosts: list[tuple[str, int] | str] | None = None,
        token: str | None = None,
        prefix: str = "asgi:",
        expiry: int = 60,
        capacity: int = 100,
        channel_capacity: dict[str, int] | None = None,
    ):
        if hosts:
            first = hosts[0]
            if isinstance(first, tuple):
                self.host, self.port = first[0], first[1]
            elif isinstance(first, str):
                parts = first.replace("http://", "").replace("https://", "").split(":")
                self.host = parts[0]
                self.port = int(parts[1]) if len(parts) > 1 else 16379
            else:
                self.host, self.port = host, port
        else:
            self.host = host
            self.port = port

        self.token = token
        self.prefix = prefix
        self.expiry = expiry
        self.capacity = capacity
        self.channel_capacity = channel_capacity or {}
        self._clients: dict[asyncio.AbstractEventLoop, Any] = {}

    def _get_capacity(self, channel: str) -> int:
        return self.channel_capacity.get(channel, self.capacity)

    async def _get_client(self) -> Any:
        loop = asyncio.get_running_loop()
        client = self._clients.get(loop)
        if client is None:
            from ._aerokv import Client  # type: ignore
            client = await Client.connect(
                self.host,
                self.port,
                token=self.token,
                prefix=self.prefix,
            )
            self._clients[loop] = client
        return client

    def _validate_name(self, name: str, name_type: str = "channel") -> None:
        if not name:
            raise TypeError(f"{name_type} name must not be empty")
        valid_chars = set(string.ascii_letters + string.digits + "_-.")
        # Specific channel names can also contain '!'
        if name_type == "channel":
            valid_chars.add("!")
        if any(c not in valid_chars for c in name):
            raise TypeError(
                f"{name_type.capitalize()} name '{name}' contains invalid characters. "
                f"Only ASCII alphanumerics, hyphens, underscores, and dots are allowed."
            )

    async def send(self, channel: str, message: dict) -> None:
        """
        Send a message to the given channel name.
        Raises ChannelFull if the channel queue capacity is exceeded.
        """
        self._validate_name(channel, "channel")
        if not isinstance(message, dict):
            raise TypeError("Message must be a dict")

        client = await self._get_client()
        payload = _serialize(message)
        capacity = self._get_capacity(channel)
        ttl_ms = int(self.expiry * 1000)

        delivered = await client.chan_send(channel, payload, capacity=capacity, ttl_ms=ttl_ms)
        if not delivered:
            raise ChannelFull(f"Channel {channel} is at capacity ({capacity} messages)")

    async def receive(self, channel: str) -> dict:
        """
        Receive the next available message for the given channel.
        Waits asynchronously until a message is delivered by the server.
        """
        self._validate_name(channel, "channel")
        client = await self._get_client()

        while True:
            raw = await client.chan_receive(channel, timeout_ms=30000)
            if raw is not None:
                return _deserialize(raw)

    async def new_channel(self, prefix: str = "specific.") -> str:
        """
        Create a new, globally unique channel name.
        """
        return f"{prefix}{uuid4().hex}!{secrets.token_hex(6)}"

    async def group_add(self, group: str, channel: str) -> None:
        """
        Add a channel to a group.
        """
        self._validate_name(group, "group")
        self._validate_name(channel, "channel")
        client = await self._get_client()
        await client.group_add(group, channel)

    async def group_discard(self, group: str, channel: str) -> None:
        """
        Remove a channel from a group.
        """
        self._validate_name(group, "group")
        self._validate_name(channel, "channel")
        client = await self._get_client()
        await client.group_discard(group, channel)

    async def group_send(self, group: str, message: dict) -> None:
        """
        Broadcast a message to all channels belonging to the group.
        """
        self._validate_name(group, "group")
        if not isinstance(message, dict):
            raise TypeError("Message must be a dict")

        client = await self._get_client()
        payload = _serialize(message)
        ttl_ms = int(self.expiry * 1000)
        await client.group_send(group, payload, capacity=self.capacity, ttl_ms=ttl_ms)

    async def flush(self) -> None:
        """
        Flush all state in AeroKV.
        """
        client = await self._get_client()
        await client.flush()

