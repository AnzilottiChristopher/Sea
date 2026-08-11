#include <stddef.h>

typedef struct {
    unsigned long start;
    unsigned long end;
    char **lines;
} thread_arg;

void trim_newline(char **s) {}

void *count_words(void *args) {
    thread_arg *targ = (thread_arg *)args;
    for (unsigned long i = targ->start; i < targ->end; i++) {
        trim_newline(&targ->lines[i]);
    }
    return NULL;
}
