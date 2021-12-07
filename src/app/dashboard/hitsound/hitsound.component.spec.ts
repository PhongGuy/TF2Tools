import { ComponentFixture, TestBed } from '@angular/core/testing';

import { HitsoundComponent } from './hitsound.component';

describe('HitsoundComponent', () => {
  let component: HitsoundComponent;
  let fixture: ComponentFixture<HitsoundComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      declarations: [ HitsoundComponent ]
    })
    .compileComponents();
  });

  beforeEach(() => {
    fixture = TestBed.createComponent(HitsoundComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });
});
