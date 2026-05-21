#include <stdlib.h>

int *foo() {
  int *p = malloc(sizeof(int));
  free(p);
  return p;
}

int main(int argc, char *argv[]) { return 0; }
