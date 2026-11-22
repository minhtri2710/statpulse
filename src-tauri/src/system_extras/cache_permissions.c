// Extended-ACL trust check for cache cleanup targets.
// Returns 1 when no allow entry grants a write-class permission, 0 when one
// does, and -1 when the ACL could not be fully evaluated (fail closed).
#include <errno.h>
#include <sys/acl.h>

int statpulse_cache_acl_safe(int fd) {
  acl_t a = acl_get_fd_np(fd, ACL_TYPE_EXTENDED);
  if (!a) {
    return errno == ENOENT ? 1 : -1;
  }
  const acl_permset_mask_t writes = ACL_WRITE_DATA | ACL_APPEND_DATA |
                                    ACL_DELETE | ACL_DELETE_CHILD |
                                    ACL_WRITE_ATTRIBUTES |
                                    ACL_WRITE_EXTATTRIBUTES |
                                    ACL_WRITE_SECURITY | ACL_CHANGE_OWNER;
  acl_entry_t e;
  int ok = 1;
  for (int which = ACL_FIRST_ENTRY;; which = ACL_NEXT_ENTRY) {
    if (acl_get_entry(a, which, &e) != 0) {
      if (errno != EINVAL) ok = -1;
      break;
    }
    acl_tag_t tag;
    acl_permset_mask_t mask;
    if (acl_get_tag_type(e, &tag) != 0) {
      ok = -1;
      break;
    }
    if (tag == ACL_EXTENDED_DENY) continue;
    if (tag != ACL_EXTENDED_ALLOW || acl_get_permset_mask_np(e, &mask) != 0) {
      ok = -1;
      break;
    }
    if (mask & writes) {
      ok = 0;
      break;
    }
  }
  acl_free(a);
  return ok;
}
