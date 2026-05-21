#include <stdlib.h>
int main() {
  int *p = malloc(sizeof(int));
  for (int i = 0; i < 10; i++) {
    *p = i;
  }
  free(p);
  return 0;
}
