#pragma once

#include <QDBusInterface>
#include <QMap>
#include <QObject>
#include <QTimer>

/// QML bridge to the running `org.abrightd` daemon.
///
/// Exposes the enabled state (read/write) plus a few live readings, and polls
/// `Status` so the switch reflects the real state even if it is changed
/// elsewhere.
class AbrightdController : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool available READ available NOTIFY availableChanged)
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY enabledChanged)
    Q_PROPERTY(double lux READ lux NOTIFY statusChanged)
    Q_PROPERTY(double brightness READ brightness NOTIFY statusChanged)
    Q_PROPERTY(double adjustment READ adjustment NOTIFY statusChanged)

public:
    explicit AbrightdController(QObject *parent = nullptr);

    bool available() const { return m_available; }
    bool enabled() const { return m_enabled; }
    double lux() const { return m_lux; }
    double brightness() const { return m_brightness; }
    double adjustment() const { return m_adjustment; }

    void setEnabled(bool value);

    Q_INVOKABLE void refresh();

Q_SIGNALS:
    void availableChanged();
    void enabledChanged();
    void statusChanged();

private:
    void applyStatus(const QMap<QString, QString> &status);
    void setAvailable(bool value);

    QDBusInterface m_iface;
    QTimer m_timer;
    bool m_available = false;
    bool m_enabled = true;
    double m_lux = 0.0;
    double m_brightness = 0.0;
    double m_adjustment = 0.0;
};
