# taskwarrior-cachingshim

Taskwarrior powerusers have large task databases.  
Client applications use the `task` process to populate views.  
Those clients will get slower and slower as the task database grows, because there is a multiplication of heavy `task` process calls.  

This project caches `read-only` operations, cutting down the time needed to render views for client applications.  
It is a drop-in shim which impersonates the `task` process, serves cached read-only command results, and lets the write commands pass through to the real `task` process.  

## Why

If you use the useful [taskwarrior-tui]() or [vit]() Taskwarrior clients, you will notice the slow downs once you hit several thousand tasks.  
A write operation in `taskwarrior-tui` takes between 8 to 12 seconds, without hooks!  

## Install

### Dependencies
Needs taskwarrior 3.x installed. 

### Installation procedure

```sh
cargo build --release
export TASKWARRIOR_TUI_TASKWARRIOR_CLI=$PWD/target/release/taskwarrior-cachingshim
```

Setting the variable and pointing it to this shim enables the behaviour and you should observe a nice speed up.  
Unset the variable to go back to normal.  

## Configuration

All config is compiled into the binary. No runtime env vars, no config
lookup at startup.  
You may edit `src/main/resources/application.properties` and rebuild to your liking.

| key | meaning |
|---|---|
| `task.path` | the real taskwarrior binary the shim execs |
| `cache.dir` | where cached responses live, `~` expands |
| `cached_commands.<cmd>` | `true` caches that command's reads |

Cacheable commands: `_get`, `show`, bare `context`, `--version`.
A command not listed is not cached. Listing a command that is not a
cacheable read (say `cached_commands.add=true`) does nothing, no error.
`context work` is a write, not a read - it always passes through.

## Tests

```sh
cargo test
```

The e2e tests run the real taskwarrior binary against an isolated data
dir in /tmp - your own `~/.task` is never touched.  
