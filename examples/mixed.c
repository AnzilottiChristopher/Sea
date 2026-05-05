#include <stdlib.h>

int main() {
  int *p = malloc(sizeof(int));
  int *q = malloc(sizeof(int));
  int *r = malloc(sizeof(int));

  free(p);
  free(p); // double free

  *q = 5;
  free(q);
  *q = 10; // use after free

  free(r);
  r = malloc(sizeof(int)); // valid reinit
  free(r);                 // valid

  return 0;
}
