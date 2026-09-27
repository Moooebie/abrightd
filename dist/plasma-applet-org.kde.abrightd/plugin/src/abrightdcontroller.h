#pragma once

#include <QDBusInterface>
#include <QMap>
#include <QObject>
#include <QTimer>

/// QML bridge to the running `org.abrightd` daemon.
///
/// Exposes the on/off state and the global adjustment (both read/write), a
/// `pointCalibrated` flag, a `resetCalibration()` action, and a few live
/// readings.  Polls `Status` so the widget reflects the daemon even if it is
/// changed elsewhere.
class AbrightdController : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool available READ available NOTIFY availableChanged)
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY enabledChanged)
    Q_PROPERTY(double adjustment READ adjustment WRITE setAdjustment NOTIFY adjustmentChanged)
    Q_PROPERTY(bool pointCalibrated READ pointCalibrated NOTIFY pointCalibratedChanged)
    Q_PROPERTY(double lux READ lux NOTIFY statusChanged)
    Q_PROPERTY(double brightness READ brightness NOTIFY statusChanged)

public:
    explicit AbrightdController(QObject *parent = nullptr);

    bool available() const { return m_available; }
    bool enabled() const { return m_enabled; }
    double adjustment() const { return m_adjustment; }
    bool pointCalibrated() const { return m_pointCalibrated; }
    double lux() const { return m_lux; }
    double brightness() const { return m_brightness; }

    void setEnabled(bool value);
    void setAdjustment(double value);
    Q_INVOKABLE void resetCalibration();
    Q_INVOKABLE void refresh();

Q_SIGNALS:
    void availableChanged();
    void enabledChanged();
    void adjustmentChanged();
    void pointCalibratedChanged();
    void statusChanged();

private:
    void applyStatus(const QMap<QString, QString> &status);
    void setAvailable(bool value);

    QDBusInterface m_iface;
    QTimer m_timer;
    bool m_available = false;
    bool m_enabled = true;
    bool m_pointCalibrated = false;
    double m_lux = 0.0;
    double m_brightness = 0.0;

    // Optimistic value tracked while a requested change is in flight, so a
    // stale poll does not make the control jump back.
    double m_adjustment = 0.0;
    bool m_adjustmentPending = false;
    double m_pendingAdjustment = 0.0;
    qint64 m_adjustmentPendingUntilMs = 0;

    qint64 m_enabledPendingUntilMs = 0;
    bool m_enabledPending = false;
    bool m_pendingEnabledValue = false;
};
