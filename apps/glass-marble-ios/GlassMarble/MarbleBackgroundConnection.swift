import ActivityKit
import CoreLocation
import OSLog

/// Owns the low-accuracy session and the Live Activity for the current SSE
/// connection. Location values are discarded at the delegate boundary.
@MainActor
final class MarbleBackgroundConnection: NSObject, @preconcurrency CLLocationManagerDelegate {
    private static let log = Logger(subsystem: "com.wanazhar.glassmarble", category: "background")
    private lazy var location: CLLocationManager = {
        let manager = CLLocationManager()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyThreeKilometers
        manager.distanceFilter = 3_000
        manager.showsBackgroundLocationIndicator = true
        manager.pausesLocationUpdatesAutomatically = false
        return manager
    }()

    private var wantsLocation = false
    private var requestedAuthorization = false
    private var activity: Activity<MarbleConnectionActivityAttributes>?
    private var lastActivityUpdate = Date.distantPast
    private(set) var isKeepingAlive = false
    var authorizationChanged: (() -> Void)?

    var permissionDenied: Bool {
        location.authorizationStatus == .denied || location.authorizationStatus == .restricted
    }

    func setLocationNeeded(_ needed: Bool, canStart: Bool) {
        wantsLocation = needed
        if needed {
            guard canStart else { return }
            switch location.authorizationStatus {
            case .notDetermined:
                guard !requestedAuthorization else { return }
                requestedAuthorization = true
                location.requestWhenInUseAuthorization()
            case .authorizedWhenInUse, .authorizedAlways:
                guard !isKeepingAlive else { return }
                location.allowsBackgroundLocationUpdates = true
                location.startUpdatingLocation()
                isKeepingAlive = true
            default:
                break
            }
        } else if isKeepingAlive {
            stopLocation()
        }
    }

    private func stopLocation() {
        location.stopUpdatingLocation()
        location.allowsBackgroundLocationUpdates = false
        isKeepingAlive = false
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        if isKeepingAlive && permissionDenied { stopLocation() }
        if wantsLocation { authorizationChanged?() }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {}

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {}

    /// ActivityKit requires a foreground request, so use the scene's
    /// will-resign callback before iOS completes the background transition.
    func startActivity(connectedAt: Date, host: String) -> Bool {
        guard isKeepingAlive, activity == nil, ActivityAuthorizationInfo().areActivitiesEnabled else { return false }
        do {
            activity = try Activity.request(
                attributes: .init(host: host),
                content: activityContent(connectedAt: connectedAt),
                pushType: nil
            )
            lastActivityUpdate = .now
            return true
        } catch {
            Self.log.debug("Could not start connection Live Activity: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }

    func refreshActivity(connectedAt: Date) {
        guard let activity, Date.now.timeIntervalSince(lastActivityUpdate) >= 30 else { return }
        lastActivityUpdate = .now
        let content = activityContent(connectedAt: connectedAt)
        Task { await activity.update(content) }
    }

    private func activityContent(connectedAt: Date) -> ActivityContent<MarbleConnectionActivityAttributes.ContentState> {
        .init(state: .init(connectedAt: connectedAt), staleDate: .now.addingTimeInterval(60))
    }

    func endActivity() {
        guard let activity else { return }
        self.activity = nil
        Task { await activity.end(nil, dismissalPolicy: .immediate) }
    }
}
