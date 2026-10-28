"""Exercise E1 over a real loopback nREPL connection with no Python dependencies."""
import argparse
import json
import socket
import time


def encode(value):
    if isinstance(value, str):
        data = value.encode()
        return str(len(data)).encode() + b":" + data
    if isinstance(value, dict):
        return b"d" + b"".join(encode(k) + encode(v) for k, v in sorted(value.items())) + b"e"
    raise TypeError(value)


def decode(stream):
    tag = stream.read(1)
    if tag == b"l":
        values = []
        while stream.peek(1)[:1] != b"e":
            values.append(decode(stream))
        stream.read(1)
        return values
    if tag == b"d":
        values = {}
        while stream.peek(1)[:1] != b"e":
            key = decode(stream)
            values[key] = decode(stream)
        stream.read(1)
        return values
    if tag == b"i":
        digits = bytearray()
        while (char := stream.read(1)) != b"e":
            if not char:
                raise EOFError("Incomplete integer")
            digits += char
        return int(digits)
    if not tag or not tag.isdigit():
        raise EOFError(f"Invalid bencode tag: {tag!r}")
    digits = bytearray(tag)
    while (char := stream.read(1)) != b":":
        if not char or not char.isdigit():
            raise EOFError("Incomplete string length")
        digits += char
    size = int(digits)
    data = stream.read(size)
    if len(data) != size:
        raise EOFError("Incomplete string")
    return data.decode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port", type=int)
    parser.add_argument("--seconds", type=float, default=50)
    parser.add_argument("--code", default="(swap! bulwark.experiment/nrepl-evals inc)")
    args = parser.parse_args()
    count = 0
    values = []
    started = time.monotonic()
    with socket.create_connection(("127.0.0.1", args.port), timeout=10) as client:
        stream = client.makefile("rb")
        while time.monotonic() - started < args.seconds:
            client.sendall(encode({"op": "eval", "id": str(count), "code": args.code}))
            while True:
                reply = decode(stream)
                if reply.get("err") or "eval-error" in reply.get("status", []):
                    raise RuntimeError(reply)
                if "value" in reply:
                    values.append(reply["value"])
                if "done" in reply.get("status", []):
                    break
            count += 1
            time.sleep(0.1)
    print(json.dumps({"evaluations": count, "elapsed_seconds": time.monotonic() - started,
                      "last_values": values[-3:]}))


if __name__ == "__main__":
    main()
