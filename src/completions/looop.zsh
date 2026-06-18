#compdef looop

# Resolve looop's data dir the same way paths.rs does:
#   $LOOOP_DATA_DIR  or  ${XDG_STATE_HOME:-$HOME/.local/state}/looop
__looop_data_dir() {
    local default="${XDG_STATE_HOME:-$HOME/.local/state}/looop"
    print -r -- "${LOOOP_DATA_DIR:-$default}"
}

# Resolve the babysit fleet root the same way paths.rs does: a non-default
# profile (distinct LOOOP_DATA_DIR) gets its own <data>/babysit root; the
# default profile honors $BABYSIT_DIR, else ~/.babysit.
__looop_babysit_root() {
    local default="${XDG_STATE_HOME:-$HOME/.local/state}/looop"
    local data="${LOOOP_DATA_DIR:-$default}"
    if [[ "$data" != "$default" ]]; then
        print -r -- "$data/babysit"
    else
        print -r -- "${BABYSIT_DIR:-$HOME/.babysit}"
    fi
}

# Goal ids = goals/<id>.md basenames (also goals/archive/<id>.md).
__looop_goals() {
    local -a goals
    local data g name
    data=$(__looop_data_dir)
    for g in "$data"/goals/*.md(N) "$data"/goals/archive/*.md(N); do
        name=${g:t}
        name=${name%.md}
        [[ -n "$name" ]] && goals+=("$name")
    done
    (( ${#goals} )) && _describe 'goal' goals
}

# Worker session ids = looop-* under the babysit fleet root, minus the pulse.
__looop_workers() {
    local -a workers
    local root s name
    root=$(__looop_babysit_root)
    for s in "$root"/sessions/looop-*(N/); do
        name=${s:t}
        [[ "$name" == "looop-pulse" ]] && continue
        [[ -n "$name" ]] && workers+=("$name")
    done
    (( ${#workers} )) && _describe 'worker' workers
}

_looop() {
    local curcontext="$curcontext" state line
    typeset -A opt_args

    _arguments -C \
        '1: :->cmd' \
        '*:: :->args'

    case $state in
        cmd)
            local -a cmds
            cmds=(
                'run:Run the pulse in the foreground (or force ONE goal)'
                'up:Run the pulse as a detached background service'
                'down:Stop the detached pulse service'
                'tick:Run a single beat and exit (debug / cron)'
                'ls:List this profile worker sessions'
                'status:Structured snapshot of the loop state'
                'start-session:Start a worker session'
                'attach:Attach to a waiting worker'
                'kill:Terminate a worker session'
                'flag:Raise a worker attention flag'
                'unflag:Clear a worker attention flag'
                'prune:Clear finished worker corpses'
                'cost:Report LLM spend from the cost ledger'
                'config:Output shell integration (eval "$(looop config zsh)")'
                'version:Print the looop version'
                'help:Show the full design manual'
            )
            _describe 'command' cmds
            ;;
        args)
            case $words[1] in
                run)
                    (( CURRENT == 2 )) && __looop_goals
                    ;;
                attach|kill|flag|unflag)
                    (( CURRENT == 2 )) && __looop_workers
                    ;;
                ls)
                    _arguments \
                        '--json[Emit JSON instead of a table]' \
                        '(-w --watch)'{-w,--watch}'[Re-render the table continuously]' \
                        '(-n --interval)'{-n,--interval}'[Refresh interval, e.g. 2s]:duration:'
                    ;;
                status)
                    _arguments '--json[Emit JSON instead of text]'
                    ;;
                cost)
                    _arguments \
                        '1:period:(today all)' \
                        '--json[Emit JSON instead of text]'
                    ;;
                config)
                    if (( CURRENT == 2 )); then
                        local -a shells
                        shells=('zsh:Zsh integration script' 'bash:Bash integration script')
                        _describe 'shell' shells
                    fi
                    ;;
            esac
            ;;
    esac
}

compdef _looop looop
