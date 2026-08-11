#include <string.h>

void trim_newline(char **str) {
    size_t len = strlen(*str);
    if (len > 0 && (*str)[len - 1] == '\n') {
        (*str)[len - 1] = '\0';
    }
}

void single_ptr_deref(char *s) {
    *s = 'x';
}

void subscript_only(char *s) {
    s[0] = 'x';
}
