#include <stdlib.h>

void foo(int *p) {}

int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  int *q = malloc(sizeof(int));

  free(p);

  foo(p);

  foo(q);

  return 0;
}
