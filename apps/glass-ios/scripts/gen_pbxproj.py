# Generates apps/glass-ios/Glass.xcodeproj (pbxproj + scheme). Usage: python3.12 gen_pbxproj.py <apps/glass-ios> [build]
import sys
root = sys.argv[1]; BUILD = sys.argv[2] if len(sys.argv) > 2 else "1"
PRODUCTS = ["GlassModels", "GlassTransport", "GlassAuth", "GlassEvents", "GlassAPI", "GlassDesktop"]
PRODUCTS = [p for p in PRODUCTS if __import__("os").path.isdir(f"{root}/../shared/GlassClientKit/Sources/{p}")]
P = "6C1A5500000000000000"
def i(n): return f"{P}{n:04X}"
APP, UT, UIT, WID, proj = i(0x10), i(0x20), i(0x30), i(0x40), i(1)
d = dict(appRef=i(2), utRef=i(3), uitRef=i(4), plistRef=i(5), gApp=i(6), gUT=i(7), gUIT=i(8), main=i(9), products=i(0xA),
 pkg=i(0xB), projList=i(0xE), projDebug=i(0xF0), projRelease=i(0xF1),
 appSrc=i(0x11), appFw=i(0x12), appRes=i(0x13), appList=i(0x14), appDebug=i(0x15), appRelease=i(0x16),
 utSrc=i(0x21), utFw=i(0x22), utRes=i(0x23), utList=i(0x24), utDebug=i(0x25), utRelease=i(0x26), utProxy=i(0x27), utDep=i(0x28),
 uitSrc=i(0x31), uitFw=i(0x32), uitRes=i(0x33), uitList=i(0x34), uitDebug=i(0x35), uitRelease=i(0x36), uitProxy=i(0x37), uitDep=i(0x38),
 widRef=i(0x41), gWid=i(0x42), widSrc=i(0x43), widFw=i(0x44), widRes=i(0x45), widList=i(0x46), widDebug=i(0x47), widRelease=i(0x48),
 widProxy=i(0x49), widDep=i(0x4A), embed=i(0x4B), embedFile=i(0x4C), gShared=i(0x4D))
prod = {p: (i(0x100 + 2*k), i(0x101 + 2*k)) for k, p in enumerate(PRODUCTS)}  # (productRef, buildFile)
T, NL = "\t", "\n"
def phase(id_, isa, name, files=""):
    return f"\t\t{id_} /* {name} */ = {{\n\t\t\tisa = {isa};\n\t\t\tbuildActionMask = 2147483647;\n\t\t\tfiles = (\n{files}\t\t\t);\n\t\t\trunOnlyForDeploymentPostprocessing = 0;\n\t\t}};\n"
def kv(pairs): return "".join(f"\t\t\t\t{k} = {v};\n" for k, v in pairs)
common = [("ALWAYS_SEARCH_USER_PATHS","NO"),("ASSETCATALOG_COMPILER_GENERATE_SWIFT_ASSET_SYMBOL_EXTENSIONS","YES"),("CLANG_ENABLE_MODULES","YES"),
 ("CLANG_ENABLE_OBJC_ARC","YES"),("COPY_PHASE_STRIP","NO"),("CURRENT_PROJECT_VERSION",BUILD),("ENABLE_STRICT_OBJC_MSGSEND","YES"),
 ("ENABLE_USER_SCRIPT_SANDBOXING","YES"),("GCC_C_LANGUAGE_STANDARD","gnu17"),("GCC_NO_COMMON_BLOCKS","YES"),("IPHONEOS_DEPLOYMENT_TARGET","18.0"),
 ("LOCALIZATION_PREFERS_STRING_CATALOGS","YES"),("MARKETING_VERSION","0.1.0"),("SDKROOT","iphoneos"),("SWIFT_STRICT_CONCURRENCY","complete"),
 ("SWIFT_VERSION","6.0"),("TARGETED_DEVICE_FAMILY",'"1,2"')]
dbg = common + [("DEBUG_INFORMATION_FORMAT","dwarf"),("ENABLE_TESTABILITY","YES"),("GCC_OPTIMIZATION_LEVEL","0"),("ONLY_ACTIVE_ARCH","YES"),
 ("SWIFT_ACTIVE_COMPILATION_CONDITIONS",'"DEBUG $(inherited)"'),("SWIFT_OPTIMIZATION_LEVEL",'"-Onone"')]
rel = common + [("DEBUG_INFORMATION_FORMAT",'"dwarf-with-dsym"'),("ENABLE_NS_ASSERTIONS","NO"),("SWIFT_COMPILATION_MODE","wholemodule"),("VALIDATE_PRODUCT","YES")]
app = [("ASSETCATALOG_COMPILER_APPICON_NAME","AppIcon"),("ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME","AccentColor"),("CODE_SIGN_STYLE","Manual"),
 ("DEVELOPMENT_TEAM",'""'),("ENABLE_PREVIEWS","YES"),("GENERATE_INFOPLIST_FILE","NO"),("INFOPLIST_FILE","Config/Info.plist"),
 ("LD_RUNPATH_SEARCH_PATHS",'(\n\t\t\t\t\t"$(inherited)",\n\t\t\t\t\t"@executable_path/Frameworks",\n\t\t\t\t)'),
 ("PRODUCT_BUNDLE_IDENTIFIER","com.wanazhar.glass"),("PRODUCT_MODULE_NAME","Glass"),("PRODUCT_NAME","Glass"),("SWIFT_EMIT_LOC_STRINGS","YES")]
ut = [("BUNDLE_LOADER",'"$(TEST_HOST)"'),("CODE_SIGN_STYLE","Manual"),("GENERATE_INFOPLIST_FILE","YES"),("PRODUCT_BUNDLE_IDENTIFIER","com.wanazhar.glass.tests"),
 ("PRODUCT_NAME",'"$(TARGET_NAME)"'),("SWIFT_EMIT_LOC_STRINGS","NO"),("TEST_HOST",'"$(BUILT_PRODUCTS_DIR)/Glass.app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/Glass"')]
uit = [("CODE_SIGN_STYLE","Manual"),("GENERATE_INFOPLIST_FILE","YES"),("PRODUCT_BUNDLE_IDENTIFIER","com.wanazhar.glass.uitests"),
 ("PRODUCT_NAME",'"$(TARGET_NAME)"'),("SWIFT_EMIT_LOC_STRINGS","NO"),("TEST_TARGET_NAME","Glass")]
wid = [("APPLICATION_EXTENSION_API_ONLY","YES"),("CODE_SIGN_STYLE","Manual"),("DEVELOPMENT_TEAM",'""'),("GENERATE_INFOPLIST_FILE","NO"),
 ("INFOPLIST_FILE","Config/GlassWidgets-Info.plist"),
 ("LD_RUNPATH_SEARCH_PATHS",'(\n\t\t\t\t\t"$(inherited)",\n\t\t\t\t\t"@executable_path/Frameworks",\n\t\t\t\t\t"@executable_path/../../Frameworks",\n\t\t\t\t)'),
 ("PRODUCT_BUNDLE_IDENTIFIER","com.wanazhar.glass.widgets"),("PRODUCT_NAME",'"$(TARGET_NAME)"'),("SKIP_INSTALL","YES"),("SWIFT_EMIT_LOC_STRINGS","YES")]
def cfg(id_, name, pairs): return f"\t\t{id_} /* {name} */ = {{\n\t\t\tisa = XCBuildConfiguration;\n\t\t\tbuildSettings = {{\n{kv(pairs)}\t\t\t}};\n\t\t\tname = {name};\n\t\t}};\n"
def clist(id_, c, a, b): return f"\t\t{id_} /* {c} */ = {{\n\t\t\tisa = XCConfigurationList;\n\t\t\tbuildConfigurations = (\n\t\t\t\t{a} /* Debug */,\n\t\t\t\t{b} /* Release */,\n\t\t\t);\n\t\t\tdefaultConfigurationIsVisible = 0;\n\t\t\tdefaultConfigurationName = Release;\n\t\t}};\n"
def target(id_, name, lst, src, fw, res, grp, ref, ptype, pname, deps="", pkgs="", phases="", groups=""):
    return (f"\t\t{id_} /* {name} */ = {{\n\t\t\tisa = PBXNativeTarget;\n\t\t\tbuildConfigurationList = {lst};\n\t\t\tbuildPhases = (\n"
     f"\t\t\t\t{src} /* Sources */,\n\t\t\t\t{fw} /* Frameworks */,\n\t\t\t\t{res} /* Resources */,\n{phases}\t\t\t);\n\t\t\tbuildRules = (\n\t\t\t);\n"
     f"\t\t\tdependencies = (\n{deps}\t\t\t);\n\t\t\tfileSystemSynchronizedGroups = (\n\t\t\t\t{grp} /* {name} */,\n{groups}\t\t\t);\n\t\t\tname = {name};\n"
     f"\t\t\tpackageProductDependencies = (\n{pkgs}\t\t\t);\n\t\t\tproductName = {name};\n\t\t\tproductReference = {ref} /* {pname} */;\n\t\t\tproductType = \"{ptype}\";\n\t\t}};\n")
def proxy(id_, tid=APP, tname="Glass"): return f"\t\t{id_} /* PBXContainerItemProxy */ = {{\n\t\t\tisa = PBXContainerItemProxy;\n\t\t\tcontainerPortal = {proj} /* Project object */;\n\t\t\tproxyType = 1;\n\t\t\tremoteGlobalIDString = {tid};\n\t\t\tremoteInfo = {tname};\n\t\t}};\n"
def dep(id_, px, tid=APP, tname="Glass"): return f"\t\t{id_} /* PBXTargetDependency */ = {{\n\t\t\tisa = PBXTargetDependency;\n\t\t\ttarget = {tid} /* {tname} */;\n\t\t\ttargetProxy = {px} /* PBXContainerItemProxy */;\n\t\t}};\n"
embedbf = f"\t\t{d['embedFile']} /* GlassWidgets.appex in Embed Foundation Extensions */ = {{isa = PBXBuildFile; fileRef = {d['widRef']} /* GlassWidgets.appex */; settings = {{ATTRIBUTES = (RemoveHeadersOnCopy, ); }}; }};\n"
embed = (f"\t\t{d['embed']} /* Embed Foundation Extensions */ = {{\n\t\t\tisa = PBXCopyFilesBuildPhase;\n\t\t\tbuildActionMask = 2147483647;\n\t\t\tdstPath = \"\";\n\t\t\tdstSubfolderSpec = 13;\n"
 f"\t\t\tfiles = (\n\t\t\t\t{d['embedFile']} /* GlassWidgets.appex in Embed Foundation Extensions */,\n\t\t\t);\n\t\t\tname = \"Embed Foundation Extensions\";\n\t\t\trunOnlyForDeploymentPostprocessing = 0;\n\t\t}};\n")
shared = f"\t\t\t\t{d['gShared']} /* Shared */,\n"
bf = "".join(f"\t\t{b} /* {p} in Frameworks */ = {{isa = PBXBuildFile; productRef = {r} /* {p} */; }};\n" for p, (r, b) in prod.items())
fwfiles = "".join(f"\t\t\t\t{b} /* {p} in Frameworks */,\n" for p, (r, b) in prod.items())
pkgs = "".join(f"\t\t\t\t{r} /* {p} */,\n" for p, (r, b) in prod.items())
pdeps = "".join(f"\t\t{r} /* {p} */ = {{\n\t\t\tisa = XCSwiftPackageProductDependency;\n\t\t\tproductName = {p};\n\t\t}};\n" for p, (r, b) in prod.items())
fr = lambda id_, n, t: f"\t\t{id_} /* {n} */ = {{isa = PBXFileReference; explicitFileType = {t}; includeInIndex = 0; path = {n}; sourceTree = BUILT_PRODUCTS_DIR; }};\n"
sg = lambda id_, n: f"\t\t{id_} /* {n} */ = {{isa = PBXFileSystemSynchronizedRootGroup; path = {n}; sourceTree = \"<group>\"; }};\n"
ta = lambda t, test: f"\t\t\t\t\t{t} = {{\n\t\t\t\t\t\tCreatedOnToolsVersion = 26.0;\n" + (f"\t\t\t\t\t\tTestTargetID = {APP};\n" if test else "") + "\t\t\t\t\t};\n"
out = ("// !$*UTF8*$!\n{\n\tarchiveVersion = 1;\n\tclasses = {\n\t};\n\tobjectVersion = 77;\n\tobjects = {\n\n"
 f"/* Begin PBXBuildFile section */\n{bf}{embedbf}/* End PBXBuildFile section */\n\n"
 f"/* Begin PBXContainerItemProxy section */\n{proxy(d['utProxy'])}{proxy(d['uitProxy'])}{proxy(d['widProxy'], WID, 'GlassWidgets')}/* End PBXContainerItemProxy section */\n\n"
 f"/* Begin PBXCopyFilesBuildPhase section */\n{embed}/* End PBXCopyFilesBuildPhase section */\n\n"
 "/* Begin PBXFileReference section */\n" + fr(d['appRef'],"Glass.app","wrapper.application") + fr(d['utRef'],"GlassTests.xctest","wrapper.cfbundle") + fr(d['uitRef'],"GlassUITests.xctest","wrapper.cfbundle") + fr(d['widRef'],"GlassWidgets.appex",'"wrapper.app-extension"')
 + f"\t\t{d['plistRef']} /* Info.plist */ = {{isa = PBXFileReference; lastKnownFileType = text.plist.xml; path = Config/Info.plist; sourceTree = \"<group>\"; }};\n/* End PBXFileReference section */\n\n"
 "/* Begin PBXFileSystemSynchronizedRootGroup section */\n" + sg(d['gApp'],"Glass") + sg(d['gUT'],"GlassTests") + sg(d['gUIT'],"GlassUITests") + sg(d['gWid'],"GlassWidgets") + sg(d['gShared'],"Shared") + "/* End PBXFileSystemSynchronizedRootGroup section */\n\n"
 "/* Begin PBXFrameworksBuildPhase section */\n" + phase(d['appFw'],"PBXFrameworksBuildPhase","Frameworks",fwfiles) + phase(d['utFw'],"PBXFrameworksBuildPhase","Frameworks") + phase(d['uitFw'],"PBXFrameworksBuildPhase","Frameworks") + phase(d['widFw'],"PBXFrameworksBuildPhase","Frameworks") + "/* End PBXFrameworksBuildPhase section */\n\n"
 f"/* Begin PBXGroup section */\n\t\t{d['main']} = {{\n\t\t\tisa = PBXGroup;\n\t\t\tchildren = (\n\t\t\t\t{d['gApp']} /* Glass */,\n\t\t\t\t{d['gUT']} /* GlassTests */,\n\t\t\t\t{d['gUIT']} /* GlassUITests */,\n\t\t\t\t{d['gWid']} /* GlassWidgets */,\n\t\t\t\t{d['gShared']} /* Shared */,\n\t\t\t\t{d['plistRef']} /* Info.plist */,\n\t\t\t\t{d['products']} /* Products */,\n\t\t\t);\n\t\t\tsourceTree = \"<group>\";\n\t\t}};\n"
 f"\t\t{d['products']} /* Products */ = {{\n\t\t\tisa = PBXGroup;\n\t\t\tchildren = (\n\t\t\t\t{d['appRef']} /* Glass.app */,\n\t\t\t\t{d['utRef']} /* GlassTests.xctest */,\n\t\t\t\t{d['uitRef']} /* GlassUITests.xctest */,\n\t\t\t\t{d['widRef']} /* GlassWidgets.appex */,\n\t\t\t);\n\t\t\tname = Products;\n\t\t\tsourceTree = \"<group>\";\n\t\t}};\n/* End PBXGroup section */\n\n"
 "/* Begin PBXNativeTarget section */\n"
 + target(APP,"Glass",d['appList'],d['appSrc'],d['appFw'],d['appRes'],d['gApp'],d['appRef'],"com.apple.product-type.application","Glass.app",pkgs=pkgs,
          deps=f"\t\t\t\t{d['widDep']} /* PBXTargetDependency */,\n", phases=f"\t\t\t\t{d['embed']} /* Embed Foundation Extensions */,\n", groups=shared)
 + target(UT,"GlassTests",d['utList'],d['utSrc'],d['utFw'],d['utRes'],d['gUT'],d['utRef'],"com.apple.product-type.bundle.unit-test","GlassTests.xctest",deps=f"\t\t\t\t{d['utDep']} /* PBXTargetDependency */,\n")
 + target(UIT,"GlassUITests",d['uitList'],d['uitSrc'],d['uitFw'],d['uitRes'],d['gUIT'],d['uitRef'],"com.apple.product-type.bundle.ui-testing","GlassUITests.xctest",deps=f"\t\t\t\t{d['uitDep']} /* PBXTargetDependency */,\n")
 + target(WID,"GlassWidgets",d['widList'],d['widSrc'],d['widFw'],d['widRes'],d['gWid'],d['widRef'],"com.apple.product-type.app-extension","GlassWidgets.appex",groups=shared)
 + "/* End PBXNativeTarget section */\n\n"
 f"/* Begin PBXProject section */\n\t\t{proj} /* Project object */ = {{\n\t\t\tisa = PBXProject;\n\t\t\tattributes = {{\n\t\t\t\tBuildIndependentTargetsInParallel = 1;\n\t\t\t\tLastSwiftUpdateCheck = 2600;\n\t\t\t\tLastUpgradeCheck = 2600;\n\t\t\t\tTargetAttributes = {{\n"
 + ta(APP,False) + ta(UT,True) + ta(UIT,True) + ta(WID,False) +
 f"\t\t\t\t}};\n\t\t\t}};\n\t\t\tbuildConfigurationList = {d['projList']};\n\t\t\tdevelopmentRegion = en;\n\t\t\thasScannedForEncodings = 0;\n\t\t\tknownRegions = (\n\t\t\t\ten,\n\t\t\t\tBase,\n\t\t\t);\n\t\t\tmainGroup = {d['main']};\n\t\t\tminimizedProjectReferenceProxies = 1;\n"
 f"\t\t\tpackageReferences = (\n\t\t\t\t{d['pkg']} /* XCLocalSwiftPackageReference \"../shared/GlassClientKit\" */,\n\t\t\t);\n\t\t\tpreferredProjectObjectVersion = 77;\n\t\t\tproductRefGroup = {d['products']} /* Products */;\n\t\t\tprojectDirPath = \"\";\n\t\t\tprojectRoot = \"\";\n"
 f"\t\t\ttargets = (\n\t\t\t\t{APP} /* Glass */,\n\t\t\t\t{UT} /* GlassTests */,\n\t\t\t\t{UIT} /* GlassUITests */,\n\t\t\t\t{WID} /* GlassWidgets */,\n\t\t\t);\n\t\t}};\n/* End PBXProject section */\n\n"
 "/* Begin PBXResourcesBuildPhase section */\n" + phase(d['appRes'],"PBXResourcesBuildPhase","Resources") + phase(d['utRes'],"PBXResourcesBuildPhase","Resources") + phase(d['uitRes'],"PBXResourcesBuildPhase","Resources") + phase(d['widRes'],"PBXResourcesBuildPhase","Resources") + "/* End PBXResourcesBuildPhase section */\n\n"
 "/* Begin PBXSourcesBuildPhase section */\n" + phase(d['appSrc'],"PBXSourcesBuildPhase","Sources") + phase(d['utSrc'],"PBXSourcesBuildPhase","Sources") + phase(d['uitSrc'],"PBXSourcesBuildPhase","Sources") + phase(d['widSrc'],"PBXSourcesBuildPhase","Sources") + "/* End PBXSourcesBuildPhase section */\n\n"
 "/* Begin PBXTargetDependency section */\n" + dep(d['utDep'],d['utProxy']) + dep(d['uitDep'],d['uitProxy']) + dep(d['widDep'],d['widProxy'],WID,"GlassWidgets") + "/* End PBXTargetDependency section */\n\n"
 "/* Begin XCBuildConfiguration section */\n" + cfg(d['projDebug'],"Debug",dbg) + cfg(d['projRelease'],"Release",rel) + cfg(d['appDebug'],"Debug",app) + cfg(d['appRelease'],"Release",app)
 + cfg(d['utDebug'],"Debug",ut) + cfg(d['utRelease'],"Release",ut) + cfg(d['uitDebug'],"Debug",uit) + cfg(d['uitRelease'],"Release",uit) + cfg(d['widDebug'],"Debug",wid) + cfg(d['widRelease'],"Release",wid) + "/* End XCBuildConfiguration section */\n\n"
 "/* Begin XCConfigurationList section */\n" + clist(d['projList'],'Build configuration list for PBXProject "Glass"',d['projDebug'],d['projRelease'])
 + clist(d['appList'],'Build configuration list for PBXNativeTarget "Glass"',d['appDebug'],d['appRelease'])
 + clist(d['utList'],'Build configuration list for PBXNativeTarget "GlassTests"',d['utDebug'],d['utRelease'])
 + clist(d['uitList'],'Build configuration list for PBXNativeTarget "GlassUITests"',d['uitDebug'],d['uitRelease'])
 + clist(d['widList'],'Build configuration list for PBXNativeTarget "GlassWidgets"',d['widDebug'],d['widRelease']) + "/* End XCConfigurationList section */\n\n"
 f"/* Begin XCLocalSwiftPackageReference section */\n\t\t{d['pkg']} /* XCLocalSwiftPackageReference \"../shared/GlassClientKit\" */ = {{\n\t\t\tisa = XCLocalSwiftPackageReference;\n\t\t\trelativePath = ../shared/GlassClientKit;\n\t\t}};\n/* End XCLocalSwiftPackageReference section */\n\n"
 f"/* Begin XCSwiftPackageProductDependency section */\n{pdeps}/* End XCSwiftPackageProductDependency section */\n\t}};\n\trootObject = {proj} /* Project object */;\n}}\n")
open(f"{root}/Glass.xcodeproj/project.pbxproj", "w").write(out)
def ref(bid, n, b): return (f"            <BuildableReference\n               BuildableIdentifier = \"primary\"\n               BlueprintIdentifier = \"{bid}\"\n"
  f"               BuildableName = \"{n}\"\n               BlueprintName = \"{b}\"\n               ReferencedContainer = \"container:Glass.xcodeproj\">\n            </BuildableReference>")
A = ref(APP, "Glass.app", "Glass")
testable = lambda r: f"         <TestableReference\n            skipped = \"NO\"\n            parallelizable = \"NO\">\n{r}\n         </TestableReference>\n"
runnable = f"      <BuildableProductRunnable\n         runnableDebuggingMode = \"0\">\n{A}\n      </BuildableProductRunnable>\n"
scheme = ("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Scheme\n   LastUpgradeVersion = \"2600\"\n   version = \"1.7\">\n"
 "   <BuildAction\n      parallelizeBuildables = \"YES\"\n      buildImplicitDependencies = \"YES\">\n      <BuildActionEntries>\n         <BuildActionEntry\n"
 "            buildForTesting = \"YES\"\n            buildForRunning = \"YES\"\n            buildForProfiling = \"YES\"\n            buildForArchiving = \"YES\"\n            buildForAnalyzing = \"YES\">\n"
 f"{A}\n         </BuildActionEntry>\n      </BuildActionEntries>\n   </BuildAction>\n"
 "   <TestAction\n      buildConfiguration = \"Debug\"\n      selectedDebuggerIdentifier = \"Xcode.DebuggerFoundation.Debugger.LLDB\"\n      selectedLauncherIdentifier = \"Xcode.DebuggerFoundation.Launcher.LLDB\"\n      shouldUseLaunchSchemeArgsEnv = \"YES\">\n"
 "      <Testables>\n" + testable(ref(UT,"GlassTests.xctest","GlassTests")) + testable(ref(UIT,"GlassUITests.xctest","GlassUITests")) + "      </Testables>\n   </TestAction>\n"
 "   <LaunchAction\n      buildConfiguration = \"Debug\"\n      selectedDebuggerIdentifier = \"Xcode.DebuggerFoundation.Debugger.LLDB\"\n      selectedLauncherIdentifier = \"Xcode.DebuggerFoundation.Launcher.LLDB\"\n"
 "      launchStyle = \"0\"\n      useCustomWorkingDirectory = \"NO\"\n      ignoresPersistentStateOnLaunch = \"NO\"\n      debugDocumentVersioning = \"YES\"\n      debugServiceExtension = \"internal\"\n      allowLocationSimulation = \"YES\">\n"
 + runnable + "   </LaunchAction>\n   <ProfileAction\n      buildConfiguration = \"Release\"\n      shouldUseLaunchSchemeArgsEnv = \"YES\"\n      savedToolIdentifier = \"\"\n      useCustomWorkingDirectory = \"NO\"\n      debugDocumentVersioning = \"YES\">\n"
 + runnable + "   </ProfileAction>\n   <AnalyzeAction\n      buildConfiguration = \"Debug\">\n   </AnalyzeAction>\n   <ArchiveAction\n      buildConfiguration = \"Release\"\n      revealArchiveInOrganizer = \"YES\">\n   </ArchiveAction>\n</Scheme>\n")
open(f"{root}/Glass.xcodeproj/xcshareddata/xcschemes/Glass.xcscheme", "w").write(scheme)
print("generated", PRODUCTS, "build", BUILD)
