root=${0%/*}
printf '%s\n' "$*" >> "$root/calls"
if [ -f "$root/fail" ]; then cat "$root/fail" >&2; exit 1; fi
mr='{"iid":42,"title":"Fix","web_url":"https://code.company.test/group/subgroup/project/-/merge_requests/42","state":"opened","source_project_id":2,"target_project_id":1,"source_branch":"feature","target_branch":"main","references":{"full":"group/subgroup/project!42"},"detailed_merge_status":"mergeable","head_pipeline":{"id":7,"status":"running"}}'
case "$1 $2" in
  'auth status') exit 0 ;;
  'mr list') printf '[%s]\n' "$mr" ;;
  'mr view') printf '%s\n' "$mr" ;;
  'issue list') printf '[]\n' ;;
  'mr create') printf 'https://code.company.test/group/subgroup/project/-/merge_requests/42\n' ;;
  'mr merge') exit 0 ;;
  'ci get') printf '{"id":7,"status":"running","jobs":[{"id":8,"name":"test","stage":"test","status":"running"}]}\n' ;;
  'api --method') exit 0 ;;
  api*)
    case "$2" in
      */approvals) printf '{"approvals_required":1,"approved_by":[{}]}\n' ;;
      *discussions*) printf '[{"id":"discussion","notes":[{"id":2,"body":"Looks good","author":{"username":"reviewer"},"created_at":"2026-09-29T00:00:00Z"}]}]\n' ;;
      *) echo "unexpected API request" >&2; exit 2 ;;
    esac ;;
  *) echo "unexpected glab command" >&2; exit 2 ;;
esac
