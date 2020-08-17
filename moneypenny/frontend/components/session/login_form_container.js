import { connect } from 'react-redux';
import SessionForm from './session_form';
import { login } from '../../actions/session_actions';
import React from 'react'
import { Link } from 'react-router-dom'

const mapStateToProps = ({ errors }) => ({
  errors: errors.session,
  formType: 'login'
})

const mapDispatchToProps = (dispatch) => ({
  processForm: (user) => (dispatch(login(user)))
})

export default connect(mapStateToProps, mapDispatchToProps)(SessionForm)