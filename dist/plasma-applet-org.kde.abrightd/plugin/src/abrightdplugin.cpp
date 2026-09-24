#include "abrightdplugin.h"

#include "abrightdcontroller.h"

#include <qqml.h>

void AbrightdPlugin::registerTypes(const char *uri) {
    qmlRegisterType<AbrightdController>(uri, 1, 0, "Controller");
}
