#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  if (argc > 1) {
    *p = 5; // fine
  } else {
    free(p);
  }
  *p = 10; // possible use after free
  return 0;
}
