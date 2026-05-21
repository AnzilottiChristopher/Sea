#include <stdlib.h>
int main() {
  int *p = malloc(sizeof(int));
  do {
    free(p);
  } while (1);
  free(p);
  return 0;
}
