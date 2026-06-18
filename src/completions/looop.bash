# Resolve looop's data dir the same way paths.rs does:
#   $LOOOP_DATA_DIR  or  ${XDG_STATE_HOME:-$HOME/.local/state}/looop
__looop_data_dir() {
    local default="${XDG_STATE_HOME:-$HOME/.local/state}/looop"
    printf '%s' "${LOOOP_DATA_DIR:-$default}"
}

# Resolve the babysit fleet root the same way paths.rs does: a non-default
# profile (distinct LOOOP_DATA_DIR) gets its own <data>/babysit root; the
# default profile honors $BABYSIT_DIR, else ~/.babysit.
__looop_babysit_root() {
    local default="${XDG_STATE_HOME:-$HOME/.local/state}/looop"
    local data="${LOOOP_DATA_DIR:-$default}"
    if [[ "$data" != "$default" ]]; then
        printf '%s' "$data/babysit"
    else
        printf '%s' "${BABYSIT_DIR:-$HOME/.babysit}"
    fi
}

_looop() {
    local cur prev words cword
    _init_completion || return

    local subcommands="run up down tick ls status start-session attach kill flag unflag prune cost config version help"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$subcommands" -- "$cur"))
        return
    fi

    local subcmd="${words[1]}"
    [[ -z "$subcmd" ]] && return

    case "$subcmd" in
        run)
            if [[ $cword -eq 2 ]]; then
                local data goals="" g name
                data=$(__looop_data_dir)
                for g in "$data"/goals/*.md "$data"/goals/archive/*.md; do
                    [[ -f "$g" ]] || continue
                    name=$(basename "$g" .md)
                    [[ -n "$name" ]] && goals+=" $name"
                done
                COMPREPLY=($(compgen -W "$goals" -- "$cur"))
            fi
            ;;
        attach|kill|flag|unflag)
            if [[ $cword -eq 2 ]]; then
                local root workers="" s name
                root=$(__looop_babysit_root)
                for s in "$root"/sessions/looop-*/; do
                    [[ -d "$s" ]] || continue
                    name=$(basename "$s")
                    [[ "$name" == "looop-pulse" ]] && continue
                    [[ -n "$name" ]] && workers+=" $name"
                done
                COMPREPLY=($(compgen -W "$workers" -- "$cur"))
            fi
            ;;
        ls)
            COMPREPLY=($(compgen -W "--json --watch -w --interval -n" -- "$cur"))
            ;;
        status)
            COMPREPLY=($(compgen -W "--json" -- "$cur"))
            ;;
        cost)
            COMPREPLY=($(compgen -W "today all --json" -- "$cur"))
            ;;
        config)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "zsh bash" -- "$cur"))
            fi
            ;;
    esac
}
complete -F _looop looop
