#include <stdlib.h>

int main() {
  int *p = malloc(sizeof(int));

  free(p);
  p = malloc(sizeof(int)); // valid reinit after free
  free(p);                 // valid

  return 0;
}
