import { Injectable } from '@angular/core';
import { AbstractControl, ValidatorFn } from '@angular/forms';

/**
 * Validator service
 */
@Injectable({
  providedIn: 'root'
})
export class ValidatorService {

  /**
   * Creates an instance of validator service.
   */
  constructor() { }

  /**
   * Checks name
   *
   * @param cant
   * @returns name
   */
  checkName(cant: string[]): ValidatorFn {
    return (control: AbstractControl): { [key: string]: boolean } | null => {
      if (cant.filter(a => a === control.value).length !== 0) {
        return { nameExists: true };
      }
      return null;
    };
  }

  /**
   * Specials validator service
   *
   * @returns special
   */
  special(): ValidatorFn {
    return (control: AbstractControl): { [key: string]: boolean } | null => {
      const nameRegexp = /[\\/:*?"<>|]+/g;
      if (nameRegexp.test(control.value)) {
        return { special: true };
      }
      return null;
    };
  }
}
