#include <stdlib.h>
int main() {
  int *p = malloc(sizeof(int));
  if (1) {
    free(p);
  }
  free(p); // definite double free — p is MaybeFreed then freed again
  return 0;
}
