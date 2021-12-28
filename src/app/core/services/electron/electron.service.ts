import { Injectable } from '@angular/core';
import { getAppDataPath } from 'appdata-path';
import * as childProcess from 'child_process';
// If you import a module but never use any of the imported values other than as TypeScript types,
// the resulting javascript file will look as if you never imported the module at all.
import { dialog, ipcRenderer, webFrame } from 'electron';
import * as fs from 'fs-extra';
import * as http from 'http';
import * as https from 'https';
import * as path from 'path';

/**
 * Electron service
 */
@Injectable({
  providedIn: 'root'
})
export class ElectronService {
  /**
   * Ipc renderer of electron service
   */
  ipcRenderer: typeof ipcRenderer;
  /**
   * Web frame of electron service
   */
  webFrame: typeof webFrame;
  /**
   * Child process of electron service
   */
  childProcess: typeof childProcess;
  /**
   * Fs  of electron service
   */
  fs: typeof fs;
  /**
   * App data of electron service
   */
  appData: typeof getAppDataPath;
  /**
   * Path of electron service
   */
  path: typeof path;
  /**
   * Dialog electron service
   */
  dialog: typeof dialog;
  /**
   * Http of electron service
   */
  http: typeof http;
  /**
   * Https of electron service
   */
  https: typeof https;

  /**
   * Creates an instance of electron service.
   */
  constructor() {
    // Conditional imports
    if (this.isElectron) {
      this.ipcRenderer = window.require('electron').ipcRenderer;
      this.webFrame = window.require('electron').webFrame;
      this.dialog = window.require('electron').dialog;

      this.childProcess = window.require('child_process');
      this.fs = window.require('fs-extra');
      this.http = window.require('http');
      this.https = window.require('https');
      this.appData = window.require('appdata-path');
      this.path = window.require('path');

      // Notes :
      // * A NodeJS's dependency imported with 'window.require' MUST BE present in `dependencies` of both `app/package.json`
      // and `package.json (root folder)` in order to make it work here in Electron's Renderer process (src folder)
      // because it will loaded at runtime by Electron.
      // * A NodeJS's dependency imported with TS module import (ex: import { Dropbox } from 'dropbox') CAN only be present
      // in `dependencies` of `package.json (root folder)` because it is loaded during build phase and does not need to be
      // in the final bundle. Reminder : only if not used in Electron's Main process (app folder)

      // If you want to use a NodeJS 3rd party deps in Renderer process,
      // ipcRenderer.invoke can serve many common use cases.
      // https://www.electronjs.org/docs/latest/api/ipc-renderer#ipcrendererinvokechannel-args
    }
  }

  /**
   * Gets whether is electron
   */
  get isElectron(): boolean {
    return !!(window && window.process && window.process.type);
  }
}
