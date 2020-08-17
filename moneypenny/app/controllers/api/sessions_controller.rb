class Api::SessionsController < ApplicationController

  def create
    @user = User.find_by_credentials(params[:user][:email], params[:user][:password])
    if @user
      login!(@user)
      render 'api/users/show'
    else
      @user = User.new
      render json: { errors: ['Invalid username or password'] }, status: 422
    end
  end

  def destroy
    if logged_in?
      logout!
      render json: {}
    else
      render json: {errors: 'There has been an error'}, status: 404
    end
  end
end
