import asyncio
import subprocess
import time
import pytest
from aerokv import Client, AeroChannelLayer, ChannelFull


@pytest.fixture(scope="module")
def aerokv_server():
    import os
    http_port = 18888
    resp_port = 18889
    server_bin = os.path.abspath("../../target/debug/aerokv-server")

    env = os.environ.copy()
    env["AEROKV_HTTP_PORT"] = str(http_port)
    env["AEROKV_RESP_PORT"] = str(resp_port)
    env["AEROKV_CORES"] = "2"
    env["AEROKV_TOKEN"] = ""

    proc = subprocess.Popen(
        [server_bin],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    time.sleep(0.5)
    yield (http_port, resp_port)
    proc.terminate()
    proc.wait()


@pytest.mark.asyncio
async def test_kv_and_pubsub_basic(aerokv_server):
    http_port, _ = aerokv_server
    client = await Client.connect("127.0.0.1", http_port)

    # Basic KV verification
    await client.set("hello", b"world")
    assert await client.get("hello") == b"world"

    # PubSub broadcast test (0 subscribers initially)
    count = await client.publish("news", b"flash")
    assert count == 0


@pytest.mark.asyncio
async def test_channel_layer_send_receive(aerokv_server):
    http_port, _ = aerokv_server
    layer = AeroChannelLayer(host="127.0.0.1", port=http_port, capacity=10)

    channel = await layer.new_channel("test.worker.")
    message = {"type": "chat.message", "text": "hello channels", "sender": "user42"}

    # Send before receive (buffered queue)
    await layer.send(channel, message)
    received = await layer.receive(channel)
    assert received == message


@pytest.mark.asyncio
async def test_channel_layer_receive_waiter(aerokv_server):
    http_port, _ = aerokv_server
    layer = AeroChannelLayer(host="127.0.0.1", port=http_port)

    channel = await layer.new_channel("waiter.")
    expected = {"event": "wake_up", "timestamp": 123456}

    # Start receive first in background
    receive_task = asyncio.create_task(layer.receive(channel))
    await asyncio.sleep(0.05)  # Ensure receive request reached server and registered waiter

    await layer.send(channel, expected)
    result = await asyncio.wait_for(receive_task, timeout=2.0)
    assert result == expected


@pytest.mark.asyncio
async def test_channel_layer_capacity_full(aerokv_server):
    http_port, _ = aerokv_server
    layer = AeroChannelLayer(host="127.0.0.1", port=http_port, capacity=2)

    channel = await layer.new_channel("cap.")
    await layer.send(channel, {"n": 1})
    await layer.send(channel, {"n": 2})

    with pytest.raises(ChannelFull):
        await layer.send(channel, {"n": 3})

    # Pop one and send again should succeed
    msg1 = await layer.receive(channel)
    assert msg1 == {"n": 1}
    await layer.send(channel, {"n": 3})


@pytest.mark.asyncio
async def test_channel_layer_group_send(aerokv_server):
    http_port, _ = aerokv_server
    layer = AeroChannelLayer(host="127.0.0.1", port=http_port)

    group = "room_alpha"
    ch1 = await layer.new_channel("client1.")
    ch2 = await layer.new_channel("client2.")

    await layer.group_add(group, ch1)
    await layer.group_add(group, ch2)

    msg = {"type": "broadcast", "data": "welcome"}
    await layer.group_send(group, msg)

    r1 = await layer.receive(ch1)
    r2 = await layer.receive(ch2)
    assert r1 == msg
    assert r2 == msg

    # Discard ch1 and broadcast again
    await layer.group_discard(group, ch1)
    msg2 = {"type": "broadcast", "data": "ch1 left"}
    await layer.group_send(group, msg2)

    r2_next = await layer.receive(ch2)
    assert r2_next == msg2


@pytest.mark.asyncio
async def test_channel_layer_flush(aerokv_server):
    http_port, _ = aerokv_server
    layer = AeroChannelLayer(host="127.0.0.1", port=http_port)

    ch = await layer.new_channel("flush.")
    await layer.send(ch, {"test": True})
    await layer.flush()

    # Receiving after flush with short timeout should timeout because queue was wiped
    client = await layer._get_client()
    raw = await client.chan_receive(ch, timeout_ms=50)
    assert raw is None
