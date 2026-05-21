#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  switch (argc) {
  case 1:
    free(p);
    break;
  case 2:
    *p = 2;
    break;
  }
  *p = 5;
  return 0;
}
