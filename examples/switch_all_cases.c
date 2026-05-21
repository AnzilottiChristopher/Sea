#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  switch (argc) {
  case 1:
    free(p);
    break;
  case 2:
    free(p);
    break;
  default:
    free(p);
    break;
  }
  return 0;
}
