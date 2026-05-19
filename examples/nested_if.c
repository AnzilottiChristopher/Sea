#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  if (argc > 1) {
    if (argc > 2) {
      free(p);
    }
  }
  *p = 5; // possible use after free
  return 0;
}
