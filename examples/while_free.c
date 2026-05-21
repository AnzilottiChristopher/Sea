#include <stdlib.h>
int main() {
  int *p = malloc(sizeof(int));
  while (1) {
    free(p);
  }
  *p = 5;
  return 0;
}
