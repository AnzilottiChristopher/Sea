#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  switch (argc) {
  case 1:
    *p = 1;
    break;
  case 2:
    *p = 2;
    break;
  default:
    *p = 3;
    break;
  }
  free(p);
  return 0;
}
