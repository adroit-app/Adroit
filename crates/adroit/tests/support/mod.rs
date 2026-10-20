pub fn codex_script(main: &str, developer: Option<&str>) -> String {
    let reply=serde_json::json!({"id":"adroit-query","result":{"config":{"developer_instructions":developer}}}).to_string().replace('\'',"'\\''");
    format!(
        r#"if [ "$1" = app-server ]; then
    IFS= read -r initialize
    printf '%s\n' '{{"id":"adroit-init","result":{{}}}}'
    while IFS= read -r query; do
        case "$query" in
            *'"method":"config/read"'*) printf '%s\n' '{reply}'; exit 0 ;;
        esac
    done
    exit 1
fi
{main}
"#
    )
}
