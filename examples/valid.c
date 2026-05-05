#include <stdlib.h>

int main() {
  int *p = malloc(sizeof(int));
  int *q = malloc(sizeof(int));

  *p = 10;
  *q = 20;

  free(p);
  free(q);

  return 0;
}
