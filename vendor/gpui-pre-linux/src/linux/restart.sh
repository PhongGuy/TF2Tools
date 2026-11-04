# Waits for the old process ($0) to exit, then starts the app ($1) with the remaining arguments.
# A zombie has exited, and after about `limit` seconds the app starts anyway, so an old process
# nobody reaps cannot keep the updated app from opening.
limit=10
tries=0
while kill -0 "$0" 2>/dev/null; do
    read -r stat 2>/dev/null < "/proc/$0/stat" || break
    rest=${stat##*") "}
    [ "${rest%% *}" = Z ] && break
    [ "$tries" -ge $((limit * 10)) ] && break
    sleep 0.1
    tries=$((tries + 1))
done

app_path="$1"
shift
"$app_path" "$@"
