# hyle Event & Control API

Interface specification for observability tools and control systems.

## Design Principles

1. **Events flow one way** (append-only log)
2. **Control is request-response** (simple commands)
3. **Text streams are universal** (JSONL, line-based)
4. **Granularity is selectable** (filter at source)

## Architecture

```
                    ┌─────────────────────────┐
                    │        hyle             │
                    │                         │
  Control Socket ──▶│  ControlHandler         │
  (line commands)   │         │               │
                    │         ▼               │
                    │    Agent Loop           │
                    │         │               │
                    │         ▼               │
  Event Stream ◀────│    EventBus             │
  (JSONL out)       │                         │
                    └─────────────────────────┘
```

## Event Stream (Observation)

### Connection

```bash
# File-based (JSONL)
tail -f ~/.hyle/events.jsonl | jq .

# WebSocket
wscat -c ws://localhost:7071/ws/events

# TCP socket
nc localhost 7071
```

### Event Format (DevEvent)

```json
{
  "v": 1,
  "t_ms": 1704825600000,
  "session_id": "sess_abc123",
  "run_id": "run_xyz",
  "event_id": "evt_001",
  "src": "hyle",
  "kind": "tool.call",
  "seq": 42,
  "payload": { "tool": "Read", "file": "/path/to/file" }
}
```

### Event Kinds

| Kind | Description | Payload |
|------|-------------|---------|
| `session.start` | Session began | `{model, project}` |
| `session.end` | Session ended | `{success, iterations}` |
| `iteration.start` | Agent iteration began | `{iteration}` |
| `iteration.end` | Agent iteration ended | `{iteration, tool_count}` |
| `llm.turn.start` | LLM generation began | `{model, prompt_tokens}` |
| `llm.turn.end` | LLM generation ended | `{completion_tokens, tool_calls}` |
| `tool.call` | Tool invoked | `{tool, args}` |
| `tool.result` | Tool succeeded | `{tool, success, output}` |
| `tool.error` | Tool failed | `{tool, error}` |
| `stage.enter` | Entered a stage | `{name}` |
| `stage.exit` | Exited a stage | `{name, success}` |
| `model.switch` | Model changed | `{from, to, reason}` |
| `model.rate_limit` | Hit rate limit | `{model, retry_after}` |
| `contract.check` | Contract checked | `{contract_id, result}` |
| `contract.violation` | Contract violated | `{contract_id, reason}` |

### Granularity Levels

Request specific granularity when connecting:

```
GET /ws/events?level=stages     # Only stage.* events
GET /ws/events?level=tools      # stage.* + tool.*
GET /ws/events?level=turns      # stage.* + tool.* + llm.*
GET /ws/events?level=all        # Everything including tokens
```

| Level | Events | Volume |
|-------|--------|--------|
| `stages` | session, iteration, stage | ~10/min |
| `tools` | + tool calls and results | ~50/min |
| `turns` | + LLM turn boundaries | ~100/min |
| `all` | + individual tokens | ~1000/min |

## Control Socket (Commands)

### Connection

```bash
# Unix socket (preferred)
nc -U ~/.hyle/control.sock

# TCP
nc localhost 7072

# Or via API
curl -X POST localhost:7071/control -d 'status'
```

### Command Format

One command per line. Response is single JSON line.

```
COMMAND [ARGS...]
```

### Commands

#### Status & Inspection

```bash
# Get current state
status
→ {"state":"generating","iteration":3,"model":"devstral","tokens":1234}

# Get prompt history
history
→ {"messages":[{"role":"user","content":"..."},{"role":"assistant","content":"..."}]}

# Get context window contents
context
→ {"system":"...","history_tokens":800,"available":7200}

# Get current generation (partial)
generation
→ {"text":"The function should...","tokens":45,"complete":false}
```

#### Control

```bash
# Pause generation (buffer tokens, don't send)
throttle 0
→ {"ok":true,"throttle":0}

# Slow down (50% speed)
throttle 50
→ {"ok":true,"throttle":50}

# Full speed
throttle 100
→ {"ok":true,"throttle":100}

# Stop current operation gracefully
interrupt
→ {"ok":true,"interrupted":"tool.Bash"}

# Stop immediately (may lose data)
abort
→ {"ok":true,"aborted":true}

# Switch model mid-session
pivot deepseek/deepseek-r1-0528:free
→ {"ok":true,"model":"deepseek/deepseek-r1-0528:free"}
```

#### Checkpoints & Replay

```bash
# Create checkpoint
checkpoint "before refactor"
→ {"ok":true,"checkpoint_id":"chk_001","description":"before refactor"}

# List checkpoints
checkpoints
→ {"checkpoints":[{"id":"chk_001","desc":"before refactor","seq":42}]}

# Rollback to checkpoint
rollback chk_001
→ {"ok":true,"rolled_back_to":"chk_001","events_undone":15}

# Replay from checkpoint with different model
replay chk_001 --model anthropic/claude-3-haiku
→ {"ok":true,"replaying_from":"chk_001"}
```

#### Session Management

```bash
# Fork current session
fork
→ {"ok":true,"new_session_id":"sess_def456"}

# Export session
export
→ {"path":"/tmp/hyle-export-20260110.jsonl"}

# Import context from another tool
import /path/to/claude-session.jsonl
→ {"ok":true,"imported_messages":15}
```

### Error Responses

```json
{"error":"unknown_command","message":"Command 'foo' not recognized"}
{"error":"invalid_args","message":"throttle requires 0-100"}
{"error":"not_running","message":"No active generation to interrupt"}
```

## Integration Examples

### Stagehand (DAW visualization)

```bash
# Start hyle with event output
hyle --events-ws ws://localhost:7070/ws/in

# Or pipe JSONL
hyle --events-jsonl - | ./stagehand-adapter | wscat -c ws://localhost:7070/ws/in
```

### Prometheus Metrics

```bash
# hyle exports metrics at /metrics
curl localhost:7071/metrics

# Sample output:
hyle_tokens_total{model="devstral"} 12345
hyle_tool_calls_total{tool="Read"} 89
hyle_iterations_total 15
hyle_errors_total{type="rate_limit"} 3
```

### Custom Observer

```python
import websocket
import json

def on_event(ws, message):
    event = json.loads(message)
    if event['kind'] == 'tool.call':
        print(f"Tool: {event['payload']['tool']}")

ws = websocket.WebSocketApp(
    "ws://localhost:7071/ws/events?level=tools",
    on_message=on_event
)
ws.run_forever()
```

### Automated Control

```python
import socket

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect('/tmp/hyle-control.sock')

def send_command(cmd):
    sock.send(f"{cmd}\n".encode())
    return json.loads(sock.recv(4096).decode())

# Throttle when CPU is high
while True:
    cpu = get_cpu_usage()
    if cpu > 80:
        send_command("throttle 25")
    elif cpu < 50:
        send_command("throttle 100")
    time.sleep(1)
```

## Protocol Versioning

- Event format version in `v` field (currently 1)
- Control protocol version via `version` command
- Breaking changes increment major version

## Security

- Unix socket restricted by filesystem permissions
- TCP sockets bind to localhost by default
- API key not exposed through control interface
- Prompt history may contain sensitive data

## Best Practices

1. **Use stages level** for dashboards (low volume)
2. **Use tools level** for debugging (medium volume)
3. **Create checkpoints** before risky operations
4. **Throttle, don't abort** when possible
5. **Export sessions** before experiments
