#include <stdlib.h>

typedef struct {
    int value;
} thread_arg;

#define NUM_THREADS 4

int main() {
    thread_arg *args[NUM_THREADS];

    for (int i = 0; i < NUM_THREADS; i++) {
        thread_arg *arg = malloc(sizeof(thread_arg));
        arg->value = i;
        args[i] = arg;
    }

    for (int i = 0; i < NUM_THREADS; i++) {
        free(args[i]);
    }

    return 0;
}
