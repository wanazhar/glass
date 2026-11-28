import ActivityKit
import CoreLocation
import Observation
import os
import UIKit

/// The Live Activity type used up to 0.1.0 (16). Kept only so launch can end activities an older install left.
struct ConnectionActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var status: String?
    }

    var host: String?
}

/// Keeps Glass running in the background while it holds a live host connection, by asking iOS for the
/// coarsest location updates. Each location is dropped the moment it arrives: nothing is stored or sent.
@MainActor @Observable
final class KeepAlive: NSObject {
    static let shared = KeepAlive()
    static let key = "glass.keepAlive.v1"

    /// Location updates are running, so iOS keeps Glass awake in the background.
    private(set) var running = false
    /// The last time iOS refused to show the Live Activity, in its words.
    private(set) var activityError: String?
    private(set) var activitiesEnabled = ActivityAuthorizationInfo().areActivitiesEnabled
    private(set) var location: CLAuthorizationStatus = .notDetermined
    private(set) var isOn = UserDefaults.standard.bool(forKey: KeepAlive.key)
    @ObservationIgnored private let manager = CLLocationManager()
    @ObservationIgnored private var session: CLBackgroundActivitySession?
    private var holds: [String: @MainActor () -> Void] = [:]
    private var workingAgents: [String: (count: Int, oldest: Date?)] = [:]
    private(set) var activity: Activity<LensConnectionAttributes>?
    @ObservationIgnored private var authorizing: [CheckedContinuation<Void, Never>] = []
    @ObservationIgnored private var enablement: Task<Void, Never>?
    var hostName = "Glass host"

    var isConnected: Bool { !holds.isEmpty }
    private var authorized: Bool {
        [.authorizedWhenInUse, .authorizedAlways].contains(location)
    }

    override private init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyThreeKilometers
        manager.distanceFilter = CLLocationDistanceMax
        manager.pausesLocationUpdatesAutomatically = false
        manager.activityType = .other
        location = manager.authorizationStatus
        let center = NotificationCenter.default
        center.addObserver(forName: .glassDisconnect, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeepAlive.shared.disconnect() }
        }
        // iOS only accepts a new Live Activity from the foreground, so ask while Glass is on its way out,
        // not once it has reached the background.
        center.addObserver(forName: UIApplication.willResignActiveNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeepAlive.shared.startActivity() }
        }
        // Closing Glass from the app switcher ends its connections, so the Live Activity goes with it.
        center.addObserver(forName: UIApplication.willTerminateNotification, object: nil, queue: .main) { _ in
            MainActor.assumeIsolated { KeepAlive.shared.endActivitiesBeforeExit() }
        }
        // On iPad one window can close while another keeps its connection; only the last one ends the activity.
        center.addObserver(forName: UIScene.didDisconnectNotification, object: nil, queue: .main) { note in
            let closing = (note.object as AnyObject?).map(ObjectIdentifier.init)
            MainActor.assumeIsolated {
                let others = UIApplication.shared.connectedScenes.filter { ObjectIdentifier($0) != closing }
                guard others.isEmpty else { return }
                KeepAlive.shared.endActivitiesBeforeExit()
            }
        }
        // An activity left over from a previous run, or from an older install, describes a connection
        // that no longer exists.
        Task {
            for stale in Activity<LensConnectionAttributes>.activities {
                await stale.end(nil, dismissalPolicy: .immediate)
            }
            for stale in Activity<ConnectionActivityAttributes>.activities {
                await stale.end(nil, dismissalPolicy: .immediate)
            }
        }
        enablement = Task { [weak self] in
            for await enabled in ActivityAuthorizationInfo().activityEnablementUpdates {
                self?.activitiesEnabled = enabled
                if enabled { self?.update() }
            }
        }
    }

    /// Call at launch, so an activity left by a run that was closed or killed ends straight away.
    static func install() { _ = shared }

    /// How long the Live Activity counts as current without a fresh update. Glass renews it while
    /// it runs, so an activity outlasting a kill without notice shows as closed soon after.
    static let freshFor: Duration = .seconds(90)
    private var renewal: Task<Void, Never>?

    /// Turns keep-alive on or off. Turning it on asks for When In Use location; returns whether it is on.
    func set(_ on: Bool) async -> Bool {
        if on, manager.authorizationStatus == .notDetermined {
            await withCheckedContinuation { continuation in
                authorizing.append(continuation)
                manager.requestWhenInUseAuthorization()
            }
        }
        let enabled = on && authorized
        UserDefaults.standard.set(enabled, forKey: Self.key)
        isOn = enabled
        update()
        return enabled
    }

    /// A live connection opened. `stop` closes it when the Live Activity's Disconnect is tapped.
    func hold(_ key: String, stop: @escaping @MainActor () -> Void) {
        holds[key] = stop
        update()
    }

    func release(_ key: String) {
        workingAgents.removeValue(forKey: key)
        guard holds.removeValue(forKey: key) != nil else { return }
        update()
    }

    func disconnect() {
        let stops = Array(holds.values)
        holds.removeAll()
        workingAgents.removeAll()
        stops.forEach { $0() }
        update()
    }

    private func update() {
        isOn = UserDefaults.standard.bool(forKey: Self.key)
        let wanted = isOn && !holds.isEmpty && authorized
        if wanted, !running {
            manager.allowsBackgroundLocationUpdates = true
            manager.showsBackgroundLocationIndicator = true
            session = CLBackgroundActivitySession()
            manager.startUpdatingLocation()
            running = true
        } else if !wanted, running {
            manager.stopUpdatingLocation()
            session?.invalidate()
            session = nil
            manager.allowsBackgroundLocationUpdates = false
            running = false
        }
        if !wanted { endActivity() }
        else if activity == nil { startActivity() }
        else { refreshActivity() }
    }

    private var status: String {
        let conversations = holds.keys.filter { $0.hasPrefix("conversation:") }.count
        let desktop = holds["desktop"] != nil
        switch (conversations, desktop) {
        case (0, true): return "Desktop connected"
        case (_, true): return "Connected, with desktop"
        case (1, false): return "Connected"
        default: return "Connected to \(conversations) conversations"
        }
    }

    func updateAgents(_ key: String, count: Int, oldest: Date?) {
        if count > 0 { workingAgents[key] = (count, oldest) }
        else { workingAgents.removeValue(forKey: key) }
        refreshActivity()
    }

    private func activityState(pulse: Bool) -> LensConnectionAttributes.ContentState {
        let active = holds.keys.compactMap { workingAgents[$0] }
        let count = active.reduce(0) { $0 + $1.count }
        return .init(status: status, pulse: pulse, workingAgents: count == 0 ? nil : count,
                     longestAgentStart: active.compactMap(\.oldest).min())
    }

    /// iOS only accepts a new Live Activity while Glass is in the foreground, so this runs both when a
    /// connection opens and as Glass is on its way out.
    private func startActivity() {
        activitiesEnabled = ActivityAuthorizationInfo().areActivitiesEnabled
        guard running, activity == nil, activitiesEnabled,
              UIApplication.shared.applicationState != .background else { return }
        let state = activityState(pulse: false)
        do {
            activity = try Activity.request(attributes: LensConnectionAttributes(host: hostName),
                                            content: Self.content(state), pushType: nil)
            activityError = nil
            renewal = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: Self.freshFor / 3)
                    guard !Task.isCancelled else { return }
                    self?.refreshActivity(renew: true)
                }
            }
        } catch {
            activityError = error.localizedDescription
            Self.log.error("Live Activity not started: \(error.localizedDescription, privacy: .public)")
        }
    }

    private static let log = Logger(subsystem: "com.wanazhar.glass", category: "keep-alive")

    private static func content(_ state: LensConnectionAttributes.ContentState) -> ActivityContent<LensConnectionAttributes.ContentState> {
        ActivityContent(state: state, staleDate: .now + TimeInterval(freshFor.components.seconds))
    }

    private func refreshActivity(renew: Bool = false) {
        guard let activity else { return }
        let previous = activity.content.state
        let next = activityState(pulse: previous.pulse)
        guard renew || previous != next else { return }
        var state = next
        state.pulse.toggle()
        let id = activity.id
        let content = Self.content(state)
        Task.detached {
            for current in Activity<LensConnectionAttributes>.activities where current.id == id {
                await current.update(content)
            }
        }
    }

    #if DEBUG
    func previewActivity() {
        guard activity == nil, ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        do {
            activity = try Activity.request(attributes: LensConnectionAttributes(host: "Studio Mac"),
                                            content: ActivityContent(state: .init(status: "Connected", pulse: false, workingAgents: 3,
                                                                                 longestAgentStart: .now.addingTimeInterval(-121)), staleDate: nil),
                                            pushType: nil)
        } catch {
            Self.log.error("Live Activity preview not started: \(error.localizedDescription, privacy: .public)")
        }
    }
    #endif

    /// iOS is closing Glass: end every Live Activity now, waiting briefly so it happens before the process stops.
    private func endActivitiesBeforeExit() {
        renewal?.cancel()
        renewal = nil
        activity = nil
        guard !Activity<LensConnectionAttributes>.activities.isEmpty else { return }
        let done = DispatchSemaphore(value: 0)
        Task.detached {
            for current in Activity<LensConnectionAttributes>.activities {
                await current.end(nil, dismissalPolicy: .immediate)
            }
            done.signal()
        }
        _ = done.wait(timeout: .now() + 1.5)
    }

    private func endActivity() {
        renewal?.cancel()
        renewal = nil
        guard let activity else { return }
        self.activity = nil
        let id = activity.id
        Task.detached {
            for current in Activity<LensConnectionAttributes>.activities where current.id == id {
                await current.end(nil, dismissalPolicy: .immediate)
            }
        }
    }

    fileprivate func authorizationChanged() {
        location = manager.authorizationStatus
        guard location != .notDetermined else { return }
        authorizing.forEach { $0.resume() }
        authorizing.removeAll()
        if isOn, !authorized {
            UserDefaults.standard.set(false, forKey: Self.key)
            isOn = false
        }
        update()
    }

    /// Why the background connection isn't running right now, in plain words; nil when it is, or is ready to.
    func blocker(on: Bool) -> Blocker? {
        if location == .denied || location == .restricted { return .location }
        if !on { return .off }
        if !activitiesEnabled { return .liveActivities }
        if let activityError, activity == nil { return .request(activityError) }
        return nil
    }

    enum Blocker: Equatable {
        case off, location, liveActivities, request(String)
    }
}

extension KeepAlive: CLLocationManagerDelegate {
    nonisolated func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {}
    nonisolated func locationManager(_ manager: CLLocationManager, didFailWithError error: any Error) {}
    nonisolated func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        Task { @MainActor in KeepAlive.shared.authorizationChanged() }
    }
}
