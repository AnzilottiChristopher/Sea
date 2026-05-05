#include <stdlib.h>

int main() {
  int *p = malloc(sizeof(int));

  free(p);
  *p = 5;

  return 0;
}
