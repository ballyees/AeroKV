import asyncio
import os
import subprocess
import time
from typing import List
from aerokv import AeroChannelLayer, Client


def calculate_percentiles(latencies_ms: List[float]):
    if not latencies_ms:
        return {}
    s = sorted(latencies_ms)
    n = len(s)
    return {
        "min": s[0],
        "p50": s[int(n * 0.50)],
        "p90": s[int(n * 0.90)],
        "p95": s[int(n * 0.95)],
        "p99": s[int(n * 0.99)],
        "max": s[-1],
        "avg": sum(s) / n,
    }


async def benchmark_p2p_latency_and_throughput(layer: AeroChannelLayer, num_messages: int = 5_000):
    channel = await layer.new_channel("bench.p2p.")
    latencies = []
    received_count = 0

    async def consumer():
        nonlocal received_count
        while received_count < num_messages:
            msg = await layer.receive(channel)
            recv_time = time.perf_counter()
            sent_time = msg["ts"]
            latencies.append((recv_time - sent_time) * 1000.0)  # to ms
            received_count += 1

    consumer_task = asyncio.create_task(consumer())
    await asyncio.sleep(0.05)  # Let consumer register waiter

    start_time = time.perf_counter()
    for i in range(num_messages):
        msg = {"id": i, "ts": time.perf_counter(), "payload": "AeroKV channel message payload"}
        await layer.send(channel, msg)

    await consumer_task
    total_time = time.perf_counter() - start_time
    qps = num_messages / total_time
    pcts = calculate_percentiles(latencies)

    return {
        "test": "P2P Send -> Receive",
        "total_messages": num_messages,
        "total_time_sec": total_time,
        "throughput_qps": qps,
        "latency_ms": pcts,
    }


async def benchmark_concurrent_workers(layer: AeroChannelLayer, num_workers: int = 10, msgs_per_worker: int = 500):
    all_latencies = []
    start_time = time.perf_counter()

    async def worker(worker_id: int):
        channel = await layer.new_channel(f"worker.{worker_id}.")
        latencies = []

        async def sub():
            for _ in range(msgs_per_worker):
                msg = await layer.receive(channel)
                latencies.append((time.perf_counter() - msg["ts"]) * 1000.0)

        sub_task = asyncio.create_task(sub())
        await asyncio.sleep(0.02)

        for i in range(msgs_per_worker):
            await layer.send(channel, {"w": worker_id, "i": i, "ts": time.perf_counter()})

        await sub_task
        return latencies

    results = await asyncio.gather(*(worker(i) for i in range(num_workers)))
    total_time = time.perf_counter() - start_time
    for r in results:
        all_latencies.extend(r)

    total_msgs = num_workers * msgs_per_worker
    qps = total_msgs / total_time
    pcts = calculate_percentiles(all_latencies)

    return {
        "test": f"Concurrent Workers ({num_workers} workers, {msgs_per_worker} msgs each)",
        "total_messages": total_msgs,
        "total_time_sec": total_time,
        "throughput_qps": qps,
        "latency_ms": pcts,
    }


async def benchmark_group_fanout(layer: AeroChannelLayer, num_subscribers: int = 10, num_broadcasts: int = 500):
    group = "bench.group.fanout"
    channels = [await layer.new_channel(f"sub.{i}.") for i in range(num_subscribers)]

    for ch in channels:
        await layer.group_add(group, ch)

    delivered_count = 0
    all_latencies = []

    async def sub_listener(ch: str):
        nonlocal delivered_count
        for _ in range(num_broadcasts):
            msg = await layer.receive(ch)
            all_latencies.append((time.perf_counter() - msg["ts"]) * 1000.0)
            delivered_count += 1

    listener_tasks = [asyncio.create_task(sub_listener(ch)) for ch in channels]
    await asyncio.sleep(0.05)

    start_time = time.perf_counter()
    for b in range(num_broadcasts):
        await layer.group_send(group, {"b": b, "ts": time.perf_counter()})

    await asyncio.gather(*listener_tasks)
    total_time = time.perf_counter() - start_time
    total_delivered = num_subscribers * num_broadcasts
    effective_qps = total_delivered / total_time
    broadcast_qps = num_broadcasts / total_time
    pcts = calculate_percentiles(all_latencies)

    return {
        "test": f"Group Fan-out (1 -> {num_subscribers} subscribers)",
        "broadcast_count": num_broadcasts,
        "total_messages_delivered": total_delivered,
        "total_time_sec": total_time,
        "broadcast_qps": broadcast_qps,
        "effective_delivery_qps": effective_qps,
        "latency_ms": pcts,
    }


async def main():
    print("================================================================")
    print("⚡ Starting AeroKV Channel Layer Benchmark")
    print("================================================================")

    # Spawn background server on release build or debug build
    server_bin = os.path.abspath("../../target/release/aerokv-server")
    if not os.path.exists(server_bin):
        server_bin = os.path.abspath("../../target/debug/aerokv-server")

    http_port = 19999
    resp_port = 19998
    env = os.environ.copy()
    env["AEROKV_HTTP_PORT"] = str(http_port)
    env["AEROKV_RESP_PORT"] = str(resp_port)
    env["AEROKV_CORES"] = "4"
    env["AEROKV_TOKEN"] = ""

    server_proc = subprocess.Popen(
        [server_bin],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(0.6)

    try:
        layer = AeroChannelLayer(host="127.0.0.1", port=http_port, capacity=10_000)

        # 1. Point-to-Point Benchmark
        print("\n▶ [1/3] Benchmarking Point-to-Point (Send -> Receive)...")
        p2p_res = await benchmark_p2p_latency_and_throughput(layer, num_messages=5_000)
        print_result(p2p_res)

        # 2. Concurrent Workers Benchmark
        print("\n▶ [2/3] Benchmarking Concurrent Workers (10 parallel channels)...")
        worker_res = await benchmark_concurrent_workers(layer, num_workers=10, msgs_per_worker=500)
        print_result(worker_res)

        # 3. Group Fan-Out Benchmark
        print("\n▶ [3/3] Benchmarking Group Fan-Out (1 broadcast -> 10 subscribers)...")
        group_res = await benchmark_group_fanout(layer, num_subscribers=10, num_broadcasts=500)
        print_fanout_result(group_res)

    finally:
        server_proc.terminate()
        server_proc.wait()


def print_result(res):
    print(f"  Test:            {res['test']}")
    print(f"  Total Messages:  {res['total_messages']:,}")
    print(f"  Total Time:      {res['total_time_sec']:.3f} s")
    print(f"  Throughput:      {res['throughput_qps']:,.1f} msgs/sec")
    lat = res["latency_ms"]
    print(f"  Latency (min):   {lat['min']:.3f} ms")
    print(f"  Latency (p50):   {lat['p50']:.3f} ms")
    print(f"  Latency (p90):   {lat['p90']:.3f} ms")
    print(f"  Latency (p95):   {lat['p95']:.3f} ms")
    print(f"  Latency (p99):   {lat['p99']:.3f} ms")
    print(f"  Latency (avg):   {lat['avg']:.3f} ms")


def print_fanout_result(res):
    print(f"  Test:            {res['test']}")
    print(f"  Broadcasts:      {res['broadcast_count']:,}")
    print(f"  Total Delivered: {res['total_messages_delivered']:,}")
    print(f"  Total Time:      {res['total_time_sec']:.3f} s")
    print(f"  Broadcast QPS:   {res['broadcast_qps']:,.1f} broadcasts/sec")
    print(f"  Delivery QPS:    {res['effective_delivery_qps']:,.1f} delivered msgs/sec")
    lat = res["latency_ms"]
    print(f"  Latency (min):   {lat['min']:.3f} ms")
    print(f"  Latency (p50):   {lat['p50']:.3f} ms")
    print(f"  Latency (p90):   {lat['p90']:.3f} ms")
    print(f"  Latency (p95):   {lat['p95']:.3f} ms")
    print(f"  Latency (p99):   {lat['p99']:.3f} ms")
    print(f"  Latency (avg):   {lat['avg']:.3f} ms")


if __name__ == "__main__":
    asyncio.run(main())

