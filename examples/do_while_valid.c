#include <stdlib.h>
int main() {
  int *p = malloc(sizeof(int));
  int i = 0;
  do {
    *p = i;
    i = i + 1;
  } while (i < 10);
  free(p);
  return 0;
}
