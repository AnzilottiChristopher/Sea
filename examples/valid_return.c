#include <stdlib.h>

int *foo() {
  int *p = malloc(sizeof(int));
  return p; // fine — heap allocated, caller is responsible for freeing
}

int main() {
  int *p = foo();
  free(p);
  return 0;
}
